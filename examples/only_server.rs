// use async_std::task;
// use futures::channel::mpsc::channel;
// use futures::{channel::mpsc::Receiver, executor, future, pin_mut, stream::StreamExt};
// use libp2p::{
//     build_development_transport, core::transport::timeout::TransportTimeout, identity, mdns::Mdns,
//     PeerId, Swarm,
// };

// use std::{
//     error::Error,
//     task::{Context, Poll},
//     time::Duration,
// };

// use dragit::p2p::behaviour::TransferBehaviour;
// use dragit::p2p::protocol::FileToSend;
// use dragit::p2p::{MyBehaviour, PeerEvent};

// async fn execute_swarm(receiver: Receiver<FileToSend>) {
//     let local_keys = identity::Keypair::generate_ed25519();
//     let local_peer_id = PeerId::from(local_keys.public());
//     println!("\nI am Peer: {:?}\n\n", local_peer_id);

//     let (sender, _receiver) = channel::<PeerEvent>(1024);

//     let mut swarm = {
//         let mdns = Mdns::new().unwrap();
//         let transfer_behaviour = TransferBehaviour::new(sender);
//         let behaviour = MyBehaviour {
//             mdns,
//             transfer_behaviour,
//         };
//         let timeout = Duration::from_secs(60);
//         let transport = TransportTimeout::with_outgoing_timeout(
//             build_development_transport(local_keys.clone()).unwrap(),
//             timeout,
//         );

//         Swarm::new(transport, behaviour, local_peer_id)
//     };

//     Swarm::listen_on(
//         &mut swarm,
//         "/ip4/0.0.0.0/tcp/0"
//             .parse()
//             .expect("Failed to parse address"),
//     )
//     .expect("Failed to listen");
//     let mut listening = false;

//     pin_mut!(receiver);
//     task::block_on(future::poll_fn(move |context: &mut Context| {
//         loop {
//             match Receiver::poll_next_unpin(&mut receiver, context) {
//                 Poll::Ready(Some(event)) => {
//                     match swarm.transfer_behaviour.push_file(event) {
//                         Ok(_) => {}
//                         Err(e) => eprintln!("{:?}", e),
//                     };
//                 }
//                 Poll::Ready(None) => println!("nothing in queue"),
//                 Poll::Pending => break,
//             };
//         }

//         loop {
//             match swarm.poll_next_unpin(context) {
//                 Poll::Ready(Some(event)) => println!("Some event main: {:?}", event),
//                 Poll::Ready(None) => {
//                     return {
//                         println!("Ready");
//                         Poll::Ready("aaa")
//                     }
//                 }
//                 Poll::Pending => {
//                     if !listening {
//                         for addr in Swarm::listeners(&swarm) {
//                             println!("Listening on {:?}", addr);
//                             listening = true;
//                         }
//                     }

//                     break;
//                 }
//             }
//         }
//         Poll::Pending
//     }));
// }

// fn main() -> Result<(), Box<dyn Error>> {
//     let (_sender, receiver) = channel::<FileToSend>(1024);

//     let future = execute_swarm(receiver);
//     executor::block_on(future);
//     Ok(())
// }
fn main() {}
