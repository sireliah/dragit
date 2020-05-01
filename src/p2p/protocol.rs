use std::sync::Arc;

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};

use std::fs::{metadata, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::task::{Context, Poll};
use std::time::Instant;
use std::{io, iter, pin::Pin};

use async_std::fs::File as AsyncFile;
use async_std::io as asyncio;
use async_std::sync::{channel, Mutex};
use async_std::task;
use futures::channel::mpsc::{Receiver, Sender};
use futures::prelude::*;
use libp2p::core::{InboundUpgrade, OutboundUpgrade, PeerId, UpgradeInfo};

use super::commands::TransferCommand;
use super::peer::PeerEvent;
use super::util::{
    add_row, check_size, get_target_path, hash_contents, spawn_read_file_job, spawn_write_file_job,
    CHUNK_SIZE,
};

#[derive(Clone, Debug)]
pub struct FileToSend {
    pub name: String,
    pub path: String,
    pub peer: PeerId,
}

impl FileToSend {
    pub fn new(path: &str, peer: &PeerId) -> Result<Self, Box<dyn Error>> {
        metadata(path)?;
        let name = Self::extract_name(path)?;
        Ok(FileToSend {
            name,
            path: path.to_string(),
            peer: peer.to_owned(),
        })
    }

    fn extract_name(path: &str) -> Result<String, Box<dyn Error>> {
        let path = Path::new(path).canonicalize()?;
        let name = path
            .file_name()
            .expect("There is no file name")
            .to_str()
            .expect("Expected a name")
            .to_string();
        Ok(name)
    }
}

#[derive(Clone, Debug)]
pub enum ProtocolEvent {
    Received(TransferPayload),
    Sent,
}

#[derive(Clone, Debug, Default)]
pub struct TransferOut {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct TransferPayload {
    pub name: String,
    pub path: String,
    pub hash: String,
    pub size_bytes: usize,
    pub sender_queue: Sender<PeerEvent>,
    pub receiver: Arc<Mutex<Receiver<TransferCommand>>>,
}

impl TransferPayload {
    pub fn check_file(&self) -> Result<(), io::Error> {
        let mut contents = vec![];
        let mut file = BufReader::new(File::open(&self.path)?);
        file.read_to_end(&mut contents).expect("Cannot read file");
        let hash_from_disk = hash_contents(&contents);
        contents.clear();

        if hash_from_disk != self.hash {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "File corrupted!",
            ))
        } else {
            Ok(())
        }
    }

    async fn notify_progress(&self, counter: usize, total_size: usize) {
        let event = PeerEvent::TransferProgress((counter, total_size));
        if let Err(e) = self.sender_queue.to_owned().send(event).await {
            eprintln!("{:?}", e);
        }
    }

    async fn notify_incoming_file_event(&self, name: &str) {
        let event = PeerEvent::FileIncoming(name.to_string());
        if let Err(e) = self.sender_queue.to_owned().send(event).await {
            eprintln!("{:?}", e);
        }
    }

    async fn block_for_answer(
        &self,
        receiver: Arc<Mutex<Receiver<TransferCommand>>>,
    ) -> TransferCommand {
        let mut r = receiver.lock().await;
        // Wait for the user to confirm the incoming file
        task::block_on(future::poll_fn(
            move |context: &mut Context| match Receiver::poll_next_unpin(&mut r, context) {
                Poll::Ready(Some(choice)) => {
                    Poll::Ready(choice)
                }
                Poll::Ready(None) => {
                    println!("Nothing to handle now");
                    Poll::Pending
                }
                Poll::Pending => Poll::Pending,
            },
        ))
    }

    async fn read_file_payload(
        &mut self,
        mut reader: asyncio::BufReader<impl AsyncRead + AsyncWrite + Send + Unpin>,
        name: String,
        size: usize,
    ) -> Result<(usize, String, String), io::Error> {
        let mut payloads: Vec<u8> = vec![];
        let (sender, receiver) = channel::<Vec<u8>>(CHUNK_SIZE * 8);

        let path = get_target_path(&name)?;
        let job = spawn_write_file_job(receiver, path.clone());

        let mut counter: usize = 0;
        let mut res: usize = 0;
        loop {
            let mut buff = vec![0u8; CHUNK_SIZE];
            match reader.read(&mut buff).await {
                Ok(n) => {
                    if n > 0 {
                        payloads.extend(&buff[..n]);
                        counter += n;
                        res += n;

                        if payloads.len() >= (CHUNK_SIZE * 8) {
                            sender.send(payloads.clone()).await;
                            task::yield_now().await;

                            payloads.clear();

                            if res >= ((size / 10) + CHUNK_SIZE * 256) {
                                self.notify_progress(counter, size).await;
                                res = 0;
                            }
                        }
                    } else {
                        sender.send(payloads.clone()).await;
                        sender.send(vec![]).await;
                        self.notify_progress(counter, size).await;
                        break;
                    }
                }
                Err(e) => panic!("Failed reading the socket {:?}", e),
            }
        }

        job.await;

        Ok((counter, path, name))
    }

    async fn read_socket(
        &mut self,
        socket: impl AsyncRead + AsyncWrite + Send + Unpin,
    ) -> Result<(), io::Error> {
        let mut reader = asyncio::BufReader::new(socket);

        let (mut name, mut hash, mut size) = ("".to_string(), "".to_string(), "".to_string());
        reader.read_line(&mut name).await?;
        reader.read_line(&mut hash).await?;
        reader.read_line(&mut size).await?;

        let (name, hash, size) = (
            name.trim().to_string(),
            hash.trim().to_string(),
            size.trim().parse::<usize>().expect("Failed parsing size"),
        );
        println!("Name: {}, Hash: {}, Size: {}", name, hash, size);

        self.notify_incoming_file_event(&name).await;
        let rec_cp = Arc::clone(&self.receiver);

        match self.block_for_answer(rec_cp).await {
            TransferCommand::Accept => {
                self.notify_progress(0, size).await;
                let (counter, path, name) = self.read_file_payload(reader, name, size).await?;
                self.name = name;
                self.hash = hash;
                self.path = path;
                self.size_bytes = counter;
                Ok(())
            }
            TransferCommand::Deny => Err(IoError::new(ErrorKind::PermissionDenied, "Rejected")),
        }
    }
}

impl UpgradeInfo for TransferPayload {
    type Info = &'static str;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once("/transfer/1.0")
    }
}

impl TransferOut {
    async fn calculate_hash(&self) -> Result<String, io::Error> {
        // TODO: implement incremental hashing for bigger files
        let file = AsyncFile::open(&self.path).await?;
        let mut buff = asyncio::BufReader::new(&file);
        let mut contents = vec![];
        buff.read_to_end(&mut contents).await?;

        // TODO: check if necessary
        // mem::drop(contents);
        // mem::drop(buff);

        Ok(hash_contents(&contents))
    }

    async fn write_socket(
        &self,
        socket: impl AsyncRead + AsyncWrite + Send + Unpin,
    ) -> Result<(), io::Error> {
        let (sender, receiver) = channel::<Vec<u8>>(CHUNK_SIZE * 8);

        println!("Name: {:?}, Path: {:?}", self.name, &self.path);

        let hash = self.calculate_hash().await?;
        let name = add_row(&self.name);
        let size = check_size(&self.path)?;
        let size_b = add_row(&size);
        let checksum = add_row(&hash);

        let mut writer = asyncio::BufWriter::new(socket);
        writer.write(&name).await?;
        writer.write(&checksum).await?;
        writer.write(&size_b).await?;

        let job = spawn_read_file_job(sender, self.path.clone());
        loop {
            match receiver.recv().await {
                Some(payload) if payload.len() > 0 => writer.write_all(&payload).await?,
                Some(_) => break,
                None => println!("rolling"),
            }
        }
        job.await;
        writer.close().await?;
        Ok(())
    }
}

impl UpgradeInfo for TransferOut {
    type Info = &'static str;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once("/transfer/1.0")
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
        async move {
            println!("Upgrade inbound");
            let start = Instant::now();
            match self.read_socket(socket).await {
                Ok(event) => event,
                Err(e) => {
                    eprintln!("Error when reading socket: {:?}", e);
                    return Err(e);
                }
            };

            println!("Finished {:?} ms", start.elapsed().as_millis());
            Ok(self)
        }
        .boxed()
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
        async move {
            println!("Upgrade outbound");
            let start = Instant::now();

            self.write_socket(socket).await?;

            println!("Finished {:?} ms", start.elapsed().as_millis());
            Ok(())
        }
        .boxed()
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
