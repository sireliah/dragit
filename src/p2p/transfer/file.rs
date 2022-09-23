use std::error::Error;
use std::fmt;
use std::fs::{metadata, File};
use std::io::{self, Read, Write};
use std::path::Path;

use async_std::fs as asyncfs;
use libp2p::core::PeerId;
use tempfile::NamedTempFile;
use walkdir::WalkDir;

use crate::p2p::transfer::directory::{MaybeTaskHandle, ZipStream};
use crate::p2p::transfer::metadata::hash_contents;
use crate::p2p::TransferType;

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
    Zip(ZipStream, MaybeTaskHandle),
    File(asyncfs::File),
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

    pub async fn get_file_stream(&self) -> Result<StreamOption, io::Error> {
        match &self.payload {
            Payload::Dir(path) => {
                let mut zip_stream = ZipStream::new(path.to_owned());
                let handle = zip_stream.take_handle();
                Ok(StreamOption::Zip(zip_stream, handle))
            }
            Payload::Text(text) => {
                let file = asyncfs::File::from(Self::create_temp_file(text)?);
                Ok(StreamOption::File(file))
            }
            Payload::File(path) => Ok(StreamOption::File(asyncfs::File::open(path).await?)),
        }
    }

    pub async fn calculate_hash(&self) -> Result<(String, u64), io::Error> {
        get_hash_from_payload(&self.payload).await
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

fn check_directory_size(path: &str) -> Result<u64, io::Error> {
    let mut total_size = 0;
    for entry in WalkDir::new(path) {
        let entry = entry?;
        let meta = metadata(entry.path())?;
        total_size += meta.len();
    }
    Ok(total_size)
}

pub async fn get_hash_from_payload(payload: &Payload) -> Result<(String, u64), io::Error> {
    match payload {
        Payload::Dir(path) => {
            let size = check_directory_size(path)?;
            // Zip internally maintains the (CRC) hash of the zipped content, no need to calculate the hash here
            Ok(("directory".to_string(), size))
        }
        Payload::File(path) => {
            let file = asyncfs::File::open(&path).await?;
            let (hash, _) = hash_contents(file).await?;
            let meta = asyncfs::metadata(path).await?;
            Ok((hash, meta.len()))
        }
        Payload::Text(text) => {
            let file = asyncfs::File::from(FileToSend::create_temp_file(text)?);
            let (hash, _) = hash_contents(file).await?;
            Ok((hash, text.len() as u64))
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
