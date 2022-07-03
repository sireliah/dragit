use std::io::{Error, ErrorKind, Result as IOResult};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_std::task::{spawn, JoinHandle};
use async_zip::error::ZipError;
use async_zip::write::{EntryOptions, ZipFileWriter};
use async_zip::Compression;
use futures::AsyncRead;
use tokio::fs::File;
use tokio::io::copy;
use tokio::io::{duplex, DuplexStream};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};
use walkdir::WalkDir;

pub type MaybeTaskHandle = Option<JoinHandle<Result<(), Error>>>;

pub struct ZipStream {
    reader: Compat<DuplexStream>,
    task_handle: MaybeTaskHandle,
}

impl ZipStream {
    pub fn new(path: String) -> ZipStream {
        let (reader, mut writer) = duplex(1024);

        let task_handle = spawn(async move {
            println!("Zip task starts");
            let mut zip = ZipFileWriter::new(&mut writer);
            let base_path = Path::new(&path).parent();

            for entry in WalkDir::new(&path) {
                let entry = entry?;
                let file_path = entry.path();
                if file_path.is_file() {
                    let rel_path = match base_path {
                        Some(base) => file_path
                            .strip_prefix(base)
                            .map_err(|err| Error::new(ErrorKind::Other, err.to_string()))?,
                        None => file_path,
                    };
                    info!("{:?}", rel_path);
                    let opts = EntryOptions::new(
                        rel_path.to_string_lossy().to_string(),
                        Compression::Deflate,
                    );
                    let mut file = File::open(file_path).await?;

                    let mut entry_writer = zip
                        .write_entry_stream(opts)
                        .await
                        .map_err(|err| zip_error(err))?;
                    copy(&mut file, &mut entry_writer).await?;
                    entry_writer.close().await.map_err(|err| zip_error(err))?;
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
