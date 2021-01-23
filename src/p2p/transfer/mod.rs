pub mod behaviour;
pub mod metadata;
pub mod protocol;

pub use behaviour::TransferBehaviour;
pub use protocol::{FileToSend, TransferOut, TransferPayload, TransferType};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/dragit.p2p.transfer.metadata.rs"));
}
