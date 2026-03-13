use std::io::{Error, Result as IOResult};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_std::channel::Sender;
use async_std::fs::{create_dir, create_dir_all, File as AsyncFile};
use async_std::io::BufReader;
use async_std::task::{spawn, JoinHandle};
use async_zip::base::read::stream::ZipFileReader;
use async_zip::base::write::ZipFileWriter;
use async_zip::error::ZipError;
use async_zip::Compression;
use async_zip::ZipEntryBuilder;
use futures::io::copy as futures_copy;
use futures::AsyncRead;
use tokio::io::{duplex, DuplexStream};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
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
        let (reader, writer) = duplex(ZIP_BUFFER_SIZE);
        let compat_writer = writer.compat_write();

        let task_handle = spawn(async move {
            let mut zip = ZipFileWriter::new(compat_writer);
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
                        .map_err(|err| Error::other(err.to_string()))?,
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
                        Self::write_file(&mut zip, path_string, file_path).await?;
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
            zip.close().await.map_err(zip_error)?;
            Ok::<(), Error>(())
        });
        let compat = reader.compat();
        ZipStream {
            reader: compat,
            task_handle: Some(task_handle),
        }
    }

    async fn write_empty_dir(
        zip: &mut ZipFileWriter<Compat<DuplexStream>>,
        rel_path: String,
    ) -> Result<(), Error> {
        let dir_path = if cfg!(windows) {
            format!(r"{}\", rel_path)
        } else {
            format!("{}/", rel_path)
        };

        let opts = ZipEntryBuilder::new(dir_path.into(), DEFAULT_COMPRESSION);
        let entry_writer = zip
            .write_entry_stream(opts)
            .await
            .map_err(zip_error)?;
        entry_writer.close().await.map_err(zip_error)?;
        Ok(())
    }

    async fn write_empty_file(
        zip: &mut ZipFileWriter<Compat<DuplexStream>>,
        rel_path: String,
    ) -> Result<(), Error> {
        // Trying to unzip the empty file at the reader end makes tokio error with "early eof".
        // This might be an async-zip bug, but as a workaround it's enough to create empty entry here.
        let opts = ZipEntryBuilder::new(rel_path.into(), DEFAULT_COMPRESSION);
        zip.write_entry_whole(opts, &[])
            .await
            .map_err(zip_error)?;
        Ok(())
    }

    async fn write_file(
        zip: &mut ZipFileWriter<Compat<DuplexStream>>,
        rel_path: String,
        file_path: &Path,
    ) -> Result<(), Error> {
        let opts = ZipEntryBuilder::new(rel_path.into(), DEFAULT_COMPRESSION);

        let mut entry_writer = zip
            .write_entry_stream(opts)
            .await
            .map_err(zip_error)?;

        let mut file = AsyncFile::open(&file_path).await?;
        futures_copy(&mut file, &mut entry_writer).await?;
        entry_writer.close().await.map_err(zip_error)?;
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

fn zip_error(err: ZipError) -> Error {
    Error::other(format!("Zip error: {err}"))
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
    let task = spawn(async move {
        let base_path = Path::new(&target_path)
            .parent()
            .unwrap_or(Path::new(&target_path));
        let mut zip = ZipFileReader::new(buf_reader);
        let mut counter: usize = 0;
        while let Some(mut reading) = zip.next_with_entry().await.map_err(zip_error)? {
            let filename = {
                let entry = reading.reader().entry();
                entry
                    .filename()
                    .as_str()
                    .map_err(|e| Error::other(format!("Invalid filename: {e:?}")))?
                    .to_owned()
            };
            let path = base_path.join(normalize_zip_path(&filename));
            if let Some(parent) = path.parent() {
                create_dir_all(parent).await?;
            }
            debug!("Unzip: {:?}", path.to_string_lossy());

            if is_zip_dir(&path) {
                debug!("Creating dir {:?}", path);
                if let Err(e) = create_dir(&path).await {
                    warn!("Could not create directory: {:?}", e);
                };
                zip = reading.skip().await.map_err(zip_error)?;
            } else {
                debug!("Creating file {:?}", path);
                let mut file = AsyncFile::create(&path).await?;
                let bytes_copied = futures_copy(reading.reader_mut(), &mut file).await?;

                let file_size = bytes_copied as usize;
                counter += file_size;

                // Limit progress events, because they seem to be to be inefficient at gtk level
                if (file_size as f32 / size as f32) > 0.01 {
                    notify_progress(&sender_queue, counter, size, &direction).await;
                }
                zip = reading.done().await.map_err(zip_error)?;
            }
        }
        Ok::<usize, Error>(counter)
    });
    Ok(task)
}

#[cfg(test)]
mod tests {
    use crate::p2p::transfer::directory::{is_zip_dir, normalize_zip_path};
    use std::path::Path;

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
