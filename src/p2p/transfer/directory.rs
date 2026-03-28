use std::io::{Error, Result as IOResult};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_channel::Sender;
use futures::io::BufReader;
use futures::AsyncRead;
use tokio::io::{duplex, DuplexStream};
use tokio::task::{spawn, JoinHandle};
use tokio_tar::{Archive, Builder};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use crate::p2p::transfer::reader::ProgressReader;

use crate::p2p::peer::Direction;
use crate::p2p::util::{notify_progress, TSocketAlias};
use crate::p2p::PeerEvent;

/// Capacity of the duplex pipe between the tar builder task and the network sender.
const DUPLEX_CHANNEL_SIZE: usize = 1024 * 512; // 512 KiB

/// Read buffer size used when opening individual source files.
#[allow(dead_code)]
const FILE_READ_BUFFER: usize = 1024 * 256; // 256 KiB

pub type MaybeTaskHandle = Option<JoinHandle<Result<(), Error>>>;

/// An `AsyncRead` that yields a tar archive of `source_path` produced on the
/// fly in a background task.  The archive is written into the writer half of a
/// `tokio::io::duplex` channel; this struct exposes the reader half.
pub struct TarStream {
    reader: Compat<DuplexStream>,
    task_handle: MaybeTaskHandle,
}

impl TarStream {
    pub fn new(source_path: String) -> TarStream {
        let (reader, writer) = duplex(DUPLEX_CHANNEL_SIZE);

        let task_handle = spawn(async move {
            let src = Path::new(&source_path);

            // Use the directory's own name as the top-level entry inside the
            // archive so the receiver unpacks into e.g. `test_dir/` rather
            // than `.`.
            let archive_name = src.file_name().map(Path::new).unwrap_or(src);

            let mut builder = Builder::new(writer);

            // Store symlinks as symlink entries rather than following them.
            // With follow_symlinks(true) (the default) a dangling symlink
            // causes fs::metadata to return NotFound, which aborts the task
            // before into_inner() can write the end-of-archive blocks,
            // producing a truncated stream and a guaranteed MD5 mismatch.
            builder.follow_symlinks(false);

            builder.append_dir_all(archive_name, src).await?;

            // Flush and write the two 512-byte end-of-archive blocks.
            builder.into_inner().await?;

            Ok::<(), Error>(())
        });

        TarStream {
            reader: reader.compat(),
            task_handle: Some(task_handle),
        }
    }

    /// Removes and returns the background task handle so the caller can await
    /// it for error propagation after the stream has been fully consumed.
    pub fn take_handle(&mut self) -> MaybeTaskHandle {
        self.task_handle.take()
    }
}

impl AsyncRead for TarStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        slice: &mut [u8],
    ) -> Poll<IOResult<usize>> {
        Pin::new(&mut self.reader).poll_read(cx, slice)
    }
}

/// Receives a tar byte stream from `buf_reader` and unpacks it into the
/// directory that is the parent of `target_path`.
///
/// Returns a `JoinHandle` that resolves to the number of bytes announced by
/// the sender (used by callers for consistency; the actual byte count is not
/// re-measured here since `unpack_in` manages I/O internally).
pub async fn untar_stream(
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

        // Stack ProgressReader on top of the socket reader (still in
        // futures::AsyncRead land) so that every byte tokio-tar reads fires
        // throttled TransferProgress events to the UI.  The compat() call
        // then crosses the boundary into tokio::AsyncRead, which Archive
        // requires.
        let progress_reader =
            ProgressReader::new(buf_reader, size, sender_queue.clone(), direction.clone());
        let compat_reader = progress_reader.compat();

        let mut archive = Archive::new(compat_reader);

        // Extracts into base_path, creating the top-level directory
        // (the archive name) automatically and handling files, empty
        // directories, and symlinks natively.
        archive.unpack(base_path).await?;

        // Final 100 % event — ensures the bar reaches the end even if the
        // last ProgressReader notification fired slightly below 100 %.
        notify_progress(&sender_queue, size, size, &direction, None).await;

        Ok::<usize, Error>(size)
    });

    Ok(task)
}
