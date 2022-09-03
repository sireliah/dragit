use std::fs::{create_dir, create_dir_all};
use std::io::{Error, ErrorKind, Result as IOResult};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_std::io::BufReader;
use async_std::task::{spawn, JoinHandle};
use async_zip::error::ZipError;
use async_zip::read::stream::ZipFileReader;
use async_zip::write::{EntryOptions, ZipFileWriter};
use async_zip::Compression;
use futures::AsyncRead;
use tokio::fs::File;
use tokio::io::{copy_buf, duplex, BufReader as TokioBufReader, DuplexStream};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use walkdir::WalkDir;

use crate::p2p::util::TSocketAlias;

const ZIP_BUFFER_SIZE: usize = 1024 * 64;

// For "Stored" compression, some files cause "A computed CRC32 value did not match the expected value." error
// I didn't figure out why yet
// Lzma was used for the most compatibility
const COMPRESSION: Compression = Compression::Lzma;

pub type MaybeTaskHandle = Option<JoinHandle<Result<(), Error>>>;

pub struct ZipStream {
    reader: Compat<DuplexStream>,
    task_handle: MaybeTaskHandle,
}

impl ZipStream {
    pub fn new(source_path: String) -> ZipStream {
        let (reader, mut writer) = duplex(ZIP_BUFFER_SIZE);

        let task_handle = spawn(async move {
            println!("Zip task starts");
            let mut zip = ZipFileWriter::new(&mut writer);
            let base_path = Path::new(&source_path).parent();

            for entry in WalkDir::new(&source_path) {
                let entry = entry?;
                let file_path = entry.path();
                info!("{:?}", file_path);
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
                info!("{:?}", rel_path);
                if file_path.is_file() {
                    Self::write_file(&mut zip, path_string, &file_path).await?;
                } else {
                    if file_path.read_dir()?.next().is_none() {
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
            format!("{}\\", rel_path)
        } else {
            format!("{}/", rel_path)
        };

        let opts = EntryOptions::new(dir_path, COMPRESSION);
        zip.write_entry_stream(opts)
            .await
            .map_err(|err| zip_error(err))?;
        Ok(())
    }

    async fn write_file(
        zip: &mut ZipFileWriter<&mut DuplexStream>,
        rel_path: String,
        file_path: &Path,
    ) -> Result<(), Error> {
        let opts = EntryOptions::new(rel_path, COMPRESSION);

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

fn zip_error(err: ZipError) -> Error {
    Error::new(ErrorKind::Other, err.to_string())
}

#[cfg(not(windows))]
fn is_zip_dir(path: &Path) -> bool {
    path.to_string_lossy().ends_with("/")
}

#[cfg(windows)]
fn is_zip_dir(path: &Path) -> bool {
    let string_path = path.to_string_lossy();
    string_path.ends_with("\\") || string_path.ends_with("\\\\")
}

pub async fn unzip_stream(
    target_path: String,
    buf_reader: BufReader<impl TSocketAlias + 'static>,
) -> Result<JoinHandle<Result<(), Error>>, Error> {
    let mut compat_reader = buf_reader.compat();

    let task = spawn(async move {
        let base_path = Path::new(&target_path);
        let mut zip = ZipFileReader::new(&mut compat_reader);
        while !zip.finished() {
            if let Some(reader) = zip.entry_reader().await.map_err(|err| zip_error(err))? {
                let entry = reader.entry();
                let path = base_path.join(entry.name());
                if let Some(parent) = path.parent() {
                    create_dir_all(parent)?;
                }
                debug!("Unzip: {:?}", path.to_string_lossy());
                if is_zip_dir(&path) {
                    info!("PATH: {:?}", path);
                    create_dir(path)?;
                } else {
                    let mut file = File::create(&path).await?;
                    reader
                        .copy_to_end_crc(&mut file, ZIP_BUFFER_SIZE)
                        .await
                        .map_err(|err| zip_error(err))?;
                }
            }
        }
        Ok::<(), Error>(())
    });
    Ok(task)
}
