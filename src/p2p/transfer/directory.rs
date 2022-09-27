use std::convert::TryFrom;
use std::io::{Error, ErrorKind, Result as IOResult};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_std::channel::Sender;
use async_std::fs::{create_dir, create_dir_all};
use async_std::io::BufReader;
use async_std::task::{spawn, JoinHandle};
use async_zip::error::ZipError;
use async_zip::read::stream::ZipFileReader;
use async_zip::write::{EntryOptions, ZipFileWriter};
use async_zip::Compression;
use futures::AsyncRead;
use tokio::fs::File;
use tokio::io::{copy_buf, duplex, AsyncReadExt, BufReader as TokioBufReader, DuplexStream};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use walkdir::WalkDir;

use crate::p2p::peer::Direction;
use crate::p2p::util::{notify_progress, TSocketAlias};
use crate::p2p::PeerEvent;

const ZIP_BUFFER_SIZE: usize = 1024 * 64;

// Slower than Stored, but more doesn't cause any CRC32 check errors
const DEFAULT_COMPRESSION: Compression = Compression::Deflate;

pub type MaybeTaskHandle = Option<JoinHandle<Result<(), Error>>>;

pub struct ZipStream {
    reader: Compat<DuplexStream>,
    task_handle: MaybeTaskHandle,
}

impl ZipStream {
    pub fn new(source_path: String) -> ZipStream {
        let (reader, mut writer) = duplex(ZIP_BUFFER_SIZE);

        let task_handle = spawn(async move {
            let mut zip = ZipFileWriter::new(&mut writer);
            let base_path = Path::new(&source_path).parent();

            for entry in WalkDir::new(&source_path) {
                let entry = entry?;
                let file_path = entry.path();
                debug!("{:?}", file_path);

                if !file_path.exists() {
                    continue;
                }

                let rel_path = match base_path {
                    Some(base) => file_path
                        .strip_prefix(base)
                        .map_err(|err| Error::new(ErrorKind::Other, err.to_string()))?,
                    None => file_path,
                };
                let path_string = rel_path
                    .to_str()
                    .unwrap_or(&rel_path.to_string_lossy())
                    .to_owned();
                debug!("{:?}", rel_path);

                // Only files and empty directories are supported for now. Symlinks are ignored.
                if file_path.is_file() {
                    if file_path.metadata()?.len() > 0 {
                        debug!("Writing file: {}", path_string);
                        Self::write_file(&mut zip, path_string, &file_path).await?;
                    } else {
                        debug!("Writing empty file: {}", path_string);
                        Self::write_empty_file(&mut zip, path_string).await?;
                    }
                } else {
                    if file_path.read_dir()?.next().is_none() {
                        debug!("Writing empty directory: {}", path_string);
                        Self::write_empty_dir(&mut zip, path_string).await?;
                    }
                }
            }
            zip.close().await.map_err(|err| zip_error(err))?;
            Ok::<(), Error>(())
        });
        let compat = reader.compat();
        ZipStream {
            reader: compat,
            task_handle: Some(task_handle),
        }
    }

    async fn write_empty_dir(
        zip: &mut ZipFileWriter<&mut DuplexStream>,
        rel_path: String,
    ) -> Result<(), Error> {
        let dir_path = if cfg!(windows) {
            format!(r"{}\", rel_path)
        } else {
            format!("{}/", rel_path)
        };

        let opts = EntryOptions::new(dir_path, DEFAULT_COMPRESSION);
        zip.write_entry_stream(opts)
            .await
            .map_err(|err| zip_error(err))?;
        Ok(())
    }

    async fn write_empty_file(
        zip: &mut ZipFileWriter<&mut DuplexStream>,
        rel_path: String,
    ) -> Result<(), Error> {
        // Trying to unzip the empty file at the reader end makes tokio error with "early eof".
        // This might be an async-zip bug, but as a workaround it's enough to create empty entry here.
        let opts = EntryOptions::new(rel_path, DEFAULT_COMPRESSION);
        zip.write_entry_whole(opts, &[])
            .await
            .map_err(|err| zip_error(err))?;
        Ok(())
    }

    async fn write_file(
        zip: &mut ZipFileWriter<&mut DuplexStream>,
        rel_path: String,
        file_path: &Path,
    ) -> Result<(), Error> {
        let compression = get_compression(&file_path).await?;
        let opts = EntryOptions::new(rel_path, compression);

        let mut entry_writer = zip
            .write_entry_stream(opts)
            .await
            .map_err(|err| zip_error(err))?;

        let mut file = File::open(&file_path).await?;
        let mut buf_reader = TokioBufReader::with_capacity(ZIP_BUFFER_SIZE, &mut file);
        copy_buf(&mut buf_reader, &mut entry_writer).await?;
        entry_writer.close().await.map_err(|err| zip_error(err))?;
        Ok(())
    }

    pub fn take_handle(&mut self) -> MaybeTaskHandle {
        let handle = &mut self.task_handle;
        let inner = handle.take();
        self.task_handle = None;
        inner
    }
}

impl AsyncRead for ZipStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        slice: &mut [u8],
    ) -> Poll<IOResult<usize>> {
        Pin::new(&mut self.reader).poll_read(cx, slice)
    }
}

async fn get_compression(path: &Path) -> Result<Compression, Error> {
    // Check the magic number of the file to detect if the file is a zip.
    // This is a remedy for the "unexpected BufError" bug when decompressing
    // a stream that contains a zip file.
    let mut buf: [u8; 4] = [0; 4];
    let mut file = File::open(&path).await?;
    file.read_exact(&mut buf).await?;
    match buf {
        [80, 75, 3, 4] => Ok(Compression::Bz),
        _ => Ok(DEFAULT_COMPRESSION),
    }
}

fn zip_error(err: ZipError) -> Error {
    Error::new(ErrorKind::Other, format!("Zip error: {}", err.to_string()))
}

fn is_zip_dir(path: &Path) -> bool {
    let path = path.to_string_lossy();
    path.ends_with(r"\") || path.ends_with("/")
}

#[cfg(not(windows))]
fn normalize_zip_path(path_name: &str) -> String {
    path_name.replace(r"\\", "/").replace(r"\", "/")
}

#[cfg(windows)]
fn normalize_zip_path(path_name: &str) -> String {
    // Windows recognizes both back and forward slashes
    path_name.to_string()
}

pub async fn unzip_stream(
    target_path: String,
    buf_reader: BufReader<impl TSocketAlias + 'static>,
    sender_queue: Sender<PeerEvent>,
    size: usize,
    direction: Direction,
) -> Result<JoinHandle<Result<usize, Error>>, Error> {
    let mut compat_reader = buf_reader.compat();

    let task = spawn(async move {
        let base_path = Path::new(&target_path);
        let mut zip = ZipFileReader::new(&mut compat_reader);
        let mut counter: usize = 0;
        while !zip.finished() {
            if let Some(reader) = zip.entry_reader().await.map_err(|err| zip_error(err))? {
                let entry = reader.entry();
                let path = base_path.join(normalize_zip_path(entry.name()));
                if let Some(parent) = path.parent() {
                    create_dir_all(parent).await?;
                }
                debug!("Unzip: {:?}", path.to_string_lossy());

                if is_zip_dir(&path) {
                    debug!("Creating dir {:?}", path);
                    create_dir(path).await?;
                } else {
                    debug!("Creating file {:?}", path);
                    let mut file = File::create(&path).await?;
                    reader
                        .copy_to_end_crc(&mut file, ZIP_BUFFER_SIZE)
                        .await
                        .map_err(|err| zip_error(err))?;

                    let meta = file.metadata().await?;
                    let file_size = usize::try_from(meta.len())
                        .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;
                    counter += file_size;

                    // Limit progress events, because they seem to be to be inefficient at gtk level
                    if (file_size as f32 / size as f32) > 0.01 {
                        notify_progress(&sender_queue, counter, size, &direction).await;
                    }
                }
            }
        }
        Ok::<usize, Error>(counter)
    });
    Ok(task)
}

#[cfg(test)]
mod tests {
    use crate::p2p::transfer::directory::{
        get_compression, is_zip_dir, normalize_zip_path, DEFAULT_COMPRESSION,
    };
    use async_zip::Compression;
    use std::path::Path;

    #[tokio::test]
    async fn test_get_compression_is_zip() {
        let path = Path::new("tests/data/test_dir/file.zip");
        let compression = get_compression(&path).await.unwrap();
        assert_eq!(compression, Compression::Bz);
    }

    #[tokio::test]
    async fn test_get_compression_epub_is_zip() {
        // Epub is internally a zip file
        let path = Path::new("tests/data/test_dir/Der_Zauberberg.epub");
        let compression = get_compression(&path).await.unwrap();
        assert_eq!(compression, Compression::Bz);
    }

    #[tokio::test]
    async fn test_get_compression_is_not_zip() {
        let path = Path::new("tests/data/file.txt");
        let compression = get_compression(&path).await.unwrap();
        assert_eq!(compression, DEFAULT_COMPRESSION);
    }

    #[cfg(not(windows))]
    #[test]
    fn test_is_zip_dir_unix() {
        let path = Path::new("this/is/a/directory/");
        assert!(is_zip_dir(path));
    }

    #[cfg(not(windows))]
    #[test]
    fn test_is_not_zip_dir_unix() {
        let path = Path::new("this/is/a/file.txt");
        assert!(!is_zip_dir(path));
    }

    #[test]
    fn test_is_zip_dir_windows() {
        let path = Path::new(r"this\is\directory\");
        assert!(is_zip_dir(path));
    }

    #[test]
    fn test_is_zip_dir_windows_double() {
        let path = Path::new(r"this\\is\\directory\\");
        assert!(is_zip_dir(path));
    }

    #[test]
    fn test_is_not_zip_dir_windows() {
        let path = Path::new(r"this\is\file.txt");
        assert!(!is_zip_dir(path));
    }

    #[cfg(not(windows))]
    #[test]
    fn test_normalize_zip_path_windows_to_unix() {
        assert_eq!(
            normalize_zip_path(r"dir\\subdir\\file.txt"),
            "dir/subdir/file.txt"
        );
        assert_eq!(
            normalize_zip_path(r"dir\subdir\file.txt"),
            "dir/subdir/file.txt"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_normalize_zip_path_unix_to_windows() {
        assert_eq!(
            normalize_zip_path("dir/subdir/file.txt"),
            "dir/subdir/file.txt"
        );
    }
}
