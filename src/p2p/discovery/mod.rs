pub mod behaviour;
pub mod handler;
pub mod protocol;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/dragit.p2p.discovery.host.rs"));
}

pub use behaviour::DiscoveryBehaviour;
pub use protocol::DiscoveryEvent;
