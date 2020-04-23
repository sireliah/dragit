use std::sync::{Arc, Mutex};

use gio::prelude::*;
use gtk::prelude::*;

use glib::Continue;
use gtk::{timeout_add, ApplicationWindow};

use futures::channel::mpsc::{Receiver, Sender};

use crate::dnd::components::{remove_expired_boxes, PeerItem};
use crate::p2p::{CurrentPeers, FileToSend, PeerEvent};

pub fn pool_peers(
    window: &ApplicationWindow,
    layout: &gtk::Box,
    file_sender: Arc<Mutex<Sender<FileToSend>>>,
    peer_receiver: Arc<Mutex<Receiver<PeerEvent>>>,
    progress_sender: glib::Sender<PeerEvent>,
) {
    let layout_weak = layout.downgrade();
    let weak_window = window.downgrade();

    timeout_add(200, move || {
        if let Some(layout_in) = layout_weak.upgrade() {
            if let Ok(p) = peer_receiver.lock().unwrap().try_next() {
                let peers: CurrentPeers = match p {
                    Some(event) => match event {
                        PeerEvent::PeersUpdated(list) => list,
                        PeerEvent::TransferProgress((value, total)) => {
                            let _ =
                                progress_sender.send(PeerEvent::TransferProgress((value, total)));
                            return Continue(true);
                        }
                        PeerEvent::FileCorrect(file_name) => {
                            let _ = progress_sender.send(PeerEvent::FileCorrect(file_name));
                            return Continue(true);
                        }
                        event => {
                            println!("Other event: {:?}", event);
                            return Continue(true);
                        }
                    },
                    None => {
                        eprintln!("Failed to get peers from the queue");
                        return Continue(true);
                    }
                };

                let children: Vec<String> = layout_in
                    .get_children()
                    .iter()
                    .map(|c| match c.get_widget_name() {
                        Some(name) => name.as_str().to_string(),
                        None => {
                            eprintln!("Failed to get widget name");
                            "".to_string()
                        }
                    })
                    .collect();

                for peer in peers.iter().filter(|p| !children.contains(&p.name)) {
                    let name: &str = &peer.name;
                    println!("Peer: {:?}", name);

                    let item = PeerItem::new(name);
                    let sender = file_sender.clone();
                    let item = item.bind_drag_and_drop(peer, sender);

                    layout_in.pack_start(&item.container, false, false, 10);
                }
                remove_expired_boxes(&layout_in, &peers);
            };
        }

        if let Some(win) = weak_window.upgrade() {
            win.show_all();
        }
        Continue(true)
    });
}
