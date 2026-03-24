use std::error::Error;
use std::fmt;
use std::fs::{metadata, File};
use std::io::{self, Read, Write};
use std::path::Path;

use crate::p2p::transfer::directory::{MaybeTaskHandle, TarStream};
use crate::p2p::TransferType;
use libp2p::core::PeerId;
use tempfile::NamedTempFile;
use tokio_util::compat::TokioAsyncReadCompatExt;

#[derive(Debug, Clone)]
pub enum Payload {
    Dir(String),
    File(String),
    Text(String),
}

impl Payload {
    pub fn new(transfer_type: TransferType, path: String) -> Result<Payload, io::Error> {
        match transfer_type {
            TransferType::File => Ok(Payload::File(path)),
            TransferType::Dir => Ok(Payload::Dir(path)),
            TransferType::Text => {
                let mut file = File::open(path)?;
                let mut contents = String::new();
                let _ = file.read_to_string(&mut contents);

                Ok(Payload::Text(contents))
            }
        }
    }
    pub fn new_for_path(path: String) -> Result<Payload, io::Error> {
        let meta = metadata(&path)?;
        if meta.is_dir() {
            Ok(Payload::Dir(path))
        } else {
            Ok(Payload::File(path))
        }
    }
}

pub enum StreamOption {
    Tar(TarStream, MaybeTaskHandle),
    File(Box<dyn futures::AsyncRead + Send + Unpin>),
}

#[derive(Debug, Clone)]
pub struct FileToSend {
    pub peer: PeerId,
    pub name: String,
    pub payload: Payload,
    pub transfer_type: TransferType,
}

impl FileToSend {
    pub fn new(peer: &PeerId, payload: Payload) -> Result<Self, Box<dyn Error>> {
        info!("Got a payload! {}", payload);

        match payload {
            Payload::Dir(path) => {
                let os_path = Path::new(&path);
                let name = match os_path.parent() {
                    Some(base) => {
                        let child_path = os_path.strip_prefix(base)?;
                        child_path.to_string_lossy().to_string()
                    }
                    None => os_path.to_string_lossy().to_string(),
                };
                Ok(FileToSend {
                    name,
                    payload: Payload::Dir(path),
                    peer: peer.to_owned(),
                    transfer_type: TransferType::Dir,
                })
            }
            Payload::File(path) => {
                let name = Self::extract_name_path(&path)?;
                let new_payload = Payload::File(path);
                Ok(FileToSend {
                    name,
                    payload: new_payload,
                    peer: peer.to_owned(),
                    transfer_type: TransferType::File,
                })
            }
            Payload::Text(text) => {
                let name = Self::extract_name_text(&text);
                Ok(FileToSend {
                    name,
                    payload: Payload::Text(text),
                    peer: peer.to_owned(),
                    transfer_type: TransferType::Text,
                })
            }
        }
    }

    /// Returns the byte size of the payload without reading file contents.
    /// For files and directories this is a metadata-only operation (stat or walkdir).
    /// For text payloads the size is derived from the in-memory string length.
    pub async fn get_size(&self) -> Result<u64, io::Error> {
        match &self.payload {
            Payload::File(path) => {
                let meta = tokio::fs::metadata(path).await?;
                Ok(meta.len())
            }
            Payload::Dir(path) => {
                fn dir_size(path: &Path) -> u64 {
                    let mut total = 0u64;
                    if let Ok(entries) = std::fs::read_dir(path) {
                        for entry in entries.flatten() {
                            let p = entry.path();
                            match std::fs::symlink_metadata(&p) {
                                Ok(m) if m.is_dir() => total += dir_size(&p),
                                Ok(m) => total += m.len(),
                                Err(e) => warn!("Can't estimate size of {:?}: {}", p, e),
                            }
                        }
                    }
                    total
                }
                Ok(dir_size(Path::new(path)))
            }
            Payload::Text(text) => Ok(text.len() as u64),
        }
    }

    pub async fn get_file_stream(&self) -> Result<StreamOption, io::Error> {
        match &self.payload {
            Payload::Dir(path) => {
                let mut tar_stream = TarStream::new(path.to_owned());
                let handle = tar_stream.take_handle();
                Ok(StreamOption::Tar(tar_stream, handle))
            }
            Payload::Text(text) => {
                let std_file = Self::create_temp_file(text)?;
                let tokio_file = tokio::fs::File::from_std(std_file);
                Ok(StreamOption::File(Box::new(tokio_file.compat())))
            }
            Payload::File(path) => {
                let tokio_file = tokio::fs::File::open(path).await?;
                Ok(StreamOption::File(Box::new(tokio_file.compat())))
            }
        }
    }

    /// Creates temporary file from text payload, so this kind of payload
    /// can be treated as file by the transfer protocol.
    pub fn create_temp_file(text: &str) -> Result<File, io::Error> {
        let mut tmp_file = NamedTempFile::new()?;
        tmp_file.write(text.as_bytes())?;
        let file = tmp_file.reopen()?;
        Ok(file)
    }

    fn extract_name_text(text: &str) -> String {
        match text.get(0..5) {
            Some(t) => format!("{} (...)", t).replace("\n", ""),
            None => "text".to_string(),
        }
    }

    fn extract_name_path(path: &str) -> Result<String, Box<dyn Error>> {
        let path = Path::new(path).canonicalize()?;
        match path.file_name() {
            Some(name) => Ok(name.to_string_lossy().to_string()),
            None => Ok("file".to_string()),
        }
    }
}

impl fmt::Display for FileToSend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FileToSend name: {}, type: {}",
            self.name, self.transfer_type
        )
    }
}

impl fmt::Display for Payload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dir(path) => write!(f, "DirPayload({})", path),
            Self::File(path) => write!(f, "FilePayload({})", path),
            Self::Text(text) => write!(f, "TextPayload({})", text.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::p2p::transfer::file::FileToSend;

    #[test]
    fn test_extract_name_text() {
        let text = "here is the text I'm sending";
        let result = FileToSend::extract_name_text(text);

        assert_eq!(result, "here  (...)");
    }
}
