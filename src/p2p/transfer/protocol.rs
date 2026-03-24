use std::fmt;
use std::fs::remove_file;
use std::io::ErrorKind;
use std::sync::Arc;

use std::time::Instant;
use std::{io, iter, pin::Pin};

use async_channel::{Receiver, Sender};
use tokio::sync::Mutex;

use futures::io as futio;
use futures::prelude::*;
use libp2p::core::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use tokio::fs::OpenOptions;

use crate::p2p::commands::TransferCommand;
use crate::p2p::peer::{Direction, PeerEvent};
use crate::p2p::transfer::directory::untar_stream;
use crate::p2p::transfer::file::{FileToSend, Payload, StreamOption};
use crate::p2p::transfer::metadata::{Answer, Metadata, Trailer};
use crate::p2p::transfer::reader::{HashingReader, ProgressReader};
use crate::p2p::util::{self, TSocketAlias};
use crate::p2p::TransferType;
use crate::user_data;

#[derive(Clone, Debug)]
pub enum ProtocolEvent {
    Received(TransferPayload),
    Sent,
}

// Outgoing transfer to remote peer
#[derive(Clone, Debug)]
pub struct TransferOut {
    pub file: FileToSend,
    pub sender_queue: Sender<PeerEvent>,
}

// Incoming transfer to current host
#[derive(Clone, Debug)]
pub struct TransferPayload {
    pub name: String,
    pub payload: Payload,
    pub hash: String,
    pub size_bytes: usize,
    pub sender_queue: Sender<PeerEvent>,
    pub receiver: Arc<Mutex<Receiver<TransferCommand>>>,
    pub target_path: Option<String>,
}

impl TransferPayload {
    pub fn cleanup(&self) -> Result<(), io::Error> {
        if let Payload::Text(_) = self.payload {
            return match &self.target_path {
                Some(target_path) => Ok(remove_file(target_path)?),
                None => {
                    warn!("Cannot remove payload, because it has no path yet.");
                    Ok(())
                }
            };
        }
        Ok(())
    }

    async fn notify_incoming_file_event(&self, meta: &Metadata) {
        let name = meta.name.to_string();
        let size = meta.size;
        let transfer_type = meta.transfer_type;
        let event = PeerEvent::FileIncoming(name, String::new(), size, transfer_type);
        util::notify(&self.sender_queue, event).await;
    }

    async fn block_for_answer(
        &self,
        receiver: Arc<Mutex<Receiver<TransferCommand>>>,
    ) -> TransferCommand {
        let r = receiver.lock().await;
        // Wait for the user to confirm the incoming file
        loop {
            match r.recv().await {
                Ok(choice) => {
                    info!("Got the choice: {:?}", choice);
                    return choice;
                }
                Err(_) => {
                    info!("Receiver closed, retrying...");
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
            }
        }
    }

    /// Stream file data from `reader` into `path`, computing an MD5 hash
    /// in-flight, then read the sender's trailer and verify the hash matches.
    /// Returns the number of bytes written.
    async fn stream_file(
        &mut self,
        path: &str,
        mut socket: impl TSocketAlias,
        size: usize,
        direction: &Direction,
    ) -> Result<usize, io::Error> {
        info!("Path: {}", path);
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(path)
            .await
            .expect("Opening failed!");

        // Wrap tokio file as futures AsyncWrite, then buffer writes to disk
        use tokio_util::compat::TokioAsyncWriteCompatExt;
        let mut buf_file = futio::BufWriter::new(file.compat_write());

        // .take(size) bounds the copy to exactly the file bytes so that the
        // socket is not read past the end of the data into the trailer region.
        // HashingReader observes every byte in that bounded window.
        // ProgressReader is stacked on top so all three run in one copy pass.
        let bounded = (&mut socket).take(size as u64);
        let hashing = HashingReader::new(bounded);
        let mut progress_reader =
            ProgressReader::new(hashing, size, self.sender_queue.clone(), direction.clone());

        let counter = futio::copy(&mut progress_reader, &mut buf_file).await?;
        buf_file.close().await?;

        util::notify_progress(&self.sender_queue, counter as usize, size, direction, None).await;

        // Recover the HashingReader from inside the ProgressReader, then
        // unwrap the Take to finalise the digest.
        let local_hash = progress_reader.into_inner().finish();
        info!("Computed local hash: {}", local_hash);

        // The sender writes a fixed-size trailer packet right after the data.
        // The socket is still open and positioned right at the trailer now.
        let sender_hash = Trailer::read(&mut socket).await?;
        info!("Received sender hash: {}", sender_hash);

        if local_hash != sender_hash {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Hash mismatch: expected {}, got {}",
                    sender_hash, local_hash
                ),
            ));
        }

        Ok(counter as usize)
    }

    async fn stream_dir(
        &self,
        path: String,
        reader: futures::io::BufReader<impl TSocketAlias + 'static>,
        size: usize,
        direction: &Direction,
    ) -> Result<usize, io::Error> {
        let sender_copy = self.sender_queue.clone();
        let task = untar_stream(path, reader, sender_copy, size, direction.clone()).await?;
        let received_bytes = task.await??;
        Ok(received_bytes)
    }

    async fn read_file_payload(
        &mut self,
        socket: impl TSocketAlias + 'static,
        meta: &Metadata,
        size: usize,
        direction: &Direction,
    ) -> Result<(usize, String), io::Error> {
        let path =
            user_data::get_target_path(&meta.get_safe_file_name(), self.target_path.as_ref())?;

        let counter = match meta.transfer_type {
            TransferType::File => self.stream_file(&path, socket, size, direction).await?,
            TransferType::Text => self.stream_file(&path, socket, size, direction).await?,
            TransferType::Dir => {
                let reader = futures::io::BufReader::new(socket);
                self.stream_dir(path.clone(), reader, size, direction)
                    .await?
            }
        };

        Ok((counter, path))
    }

    async fn read_socket(&mut self, socket: impl TSocketAlias + 'static) -> Result<(), io::Error> {
        let direction = Direction::Incoming;
        let (meta, mut socket) = Metadata::read(socket).await?;
        info!("Meta received! \n{}", meta);

        self.notify_incoming_file_event(&meta).await;
        let rec_cp = Arc::clone(&self.receiver);

        match self.block_for_answer(rec_cp).await {
            TransferCommand::Accept(hash) => {
                Answer::write(&mut socket, true, hash).await?;

                util::notify_progress(&self.sender_queue, 0, meta.size, &direction, None).await;

                let (counter, path) = match self
                    .read_file_payload(socket, &meta, meta.size, &direction)
                    .await
                {
                    Ok((counter, path)) => (counter, path),
                    Err(err) => {
                        error!("Reading payload failed: {:?}", err);
                        if err.kind() == ErrorKind::InvalidData {
                            util::notify(&self.sender_queue, PeerEvent::FileIncorrect).await;
                        } else {
                            util::notify_error(&self.sender_queue, "Reading payload failed").await;
                        }
                        return Err(err);
                    }
                };

                self.name = meta.name;
                // hash is now verified in-flight; store an empty sentinel so
                // the field stays populated for Display / callers that read it.
                self.hash = String::new();
                self.payload = Payload::new(meta.transfer_type, path.clone())?;
                self.size_bytes = counter;

                // TransferPayload needs to know where is the actual file after successful transfer
                self.target_path = Some(path);

                Ok(())
            }
            TransferCommand::Deny(hash) => {
                warn!("Denied hash: {}", hash);
                Answer::write(&mut socket, false, hash).await?;
                Err(io::Error::new(ErrorKind::PermissionDenied, "Rejected"))
            }
        }
    }
}

impl UpgradeInfo for TransferPayload {
    type Info = &'static str;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once("/transfer/1.2")
    }
}

impl TransferOut {
    async fn write_socket(&self, socket: impl TSocketAlias) -> Result<(), io::Error> {
        let direction = Direction::Outgoing;
        info!("File to send: {}", self.file);

        util::notify_waiting(&self.sender_queue).await;

        let (size, socket) = Metadata::write(&self.file, socket).await?;

        // Check if remote is willing to accept our file
        let (accepted, socket) = Answer::read(socket).await?;
        info!("File accepted? {:?}", accepted);

        if accepted {
            match self.file.get_file_stream().await? {
                StreamOption::File(file) => {
                    self.stream_data(socket, file, size, direction).await?;
                    Ok(())
                }
                StreamOption::Tar(file, task_handle) => {
                    self.stream_data(socket, file, size, direction).await?;
                    if let Some(handle) = task_handle {
                        let _ = handle.await?;
                    }
                    Ok(())
                }
            }
        } else {
            util::notify_rejected(&self.sender_queue).await;
            Ok(())
        }
    }

    /// Stream `file` to `socket`, computing an MD5 hash in-flight, then send
    /// a trailer packet containing the hash so the receiver can verify without
    /// re-reading from disk.
    async fn stream_data(
        &self,
        mut socket: impl TSocketAlias,
        file: impl AsyncRead + Unpin,
        size: usize,
        direction: Direction,
    ) -> Result<(), io::Error> {
        let mut writer = futio::BufWriter::new(&mut socket);
        util::notify_progress(&self.sender_queue, 0, size, &direction, None).await;

        // HashingReader sits between the file and the network writer so that
        // we compute the digest in the same pass as the transfer.
        let hashing = HashingReader::new(file);
        let mut reader = ProgressReader::new(hashing, size, self.sender_queue.clone(), direction);

        futio::copy(&mut reader, &mut writer).await?;
        // Flush the BufWriter's internal buffer to the socket without closing
        // the underlying connection — the trailer still needs to be written.
        writer.flush().await?;

        // Retrieve the digest now that all bytes have been written to the socket.
        let hash = reader.into_inner().finish();
        info!("Sending trailer hash: {}", hash);

        // Send the trailer so the receiver can verify without a second disk read.
        Trailer::write(&mut socket, hash).await?;

        util::notify_completed(&self.sender_queue).await;
        Ok(())
    }
}

impl UpgradeInfo for TransferOut {
    type Info = &'static str;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once("/transfer/1.2")
    }
}

impl<TSocket> InboundUpgrade<TSocket> for TransferPayload
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = TransferPayload;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(mut self, socket: TSocket, _: Self::Info) -> Self::Future {
        Box::pin(async move {
            info!("Upgrade inbound");
            let start = Instant::now();
            self.read_socket(socket).await?;

            info!("Finished {:?} ms", start.elapsed().as_millis());
            Ok(self)
        })
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for TransferOut
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = ();
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
        Box::pin(async move {
            info!("Upgrade outbound");
            let start = Instant::now();

            self.write_socket(socket).await?;

            info!("Finished {:?} ms", start.elapsed().as_millis());
            Ok(())
        })
    }
}

impl fmt::Display for TransferOut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[TransferOut] {}", self.file)
    }
}

impl fmt::Display for TransferPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TransferPayload name: {}, payload: {}, hash: {}, size: {} bytes",
            self.name, self.payload, self.hash, self.size_bytes
        )
    }
}

impl fmt::Display for ProtocolEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolEvent::Received(e) => write!(f, "Received {}", e),
            ProtocolEvent::Sent => write!(f, "Sent"),
        }
    }
}

impl From<()> for ProtocolEvent {
    fn from(_: ()) -> Self {
        ProtocolEvent::Sent
    }
}

impl From<TransferPayload> for ProtocolEvent {
    fn from(transfer: TransferPayload) -> Self {
        ProtocolEvent::Received(transfer)
    }
}
