pub mod behaviour;
pub mod directory;
pub mod file;
pub mod jobs;
pub mod metadata;
pub mod protocol;

pub use behaviour::TransferBehaviour;
pub use file::{FileToSend, Payload};
pub use protocol::{TransferOut, TransferPayload};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/dragit.p2p.transfer.metadata.rs"));
}
