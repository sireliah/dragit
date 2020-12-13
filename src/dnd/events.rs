use std::sync::{Arc, Mutex};

use gio::prelude::*;
use gtk::prelude::*;

use async_std::sync::{Receiver, Sender};

use glib::Continue;
use gtk::{timeout_add, ApplicationWindow};

use crate::dnd::components::{EmptyListItem, PeerItem};
use crate::p2p::{CurrentPeers, FileToSend, Peer, PeerEvent};

pub fn pool_peers(
    window: &ApplicationWindow,
    layout: &gtk::Box,
    file_sender: Arc<Mutex<Sender<FileToSend>>>,
    peer_receiver: Arc<Mutex<Receiver<PeerEvent>>>,
    peer_event_sender: glib::Sender<PeerEvent>,
) {
    let empty_item = EmptyListItem::new();
    layout.pack_start(&empty_item.container, false, false, 10);
    empty_item.show();

    let layout_weak = layout.downgrade();
    let weak_window = window.downgrade();

    timeout_add(200, move || {
        if let Some(layout_in) = layout_weak.upgrade() {
            let children: Vec<String> = layout_in
                .get_children()
                .iter()
                .map(|c| match c.get_widget_name() {
                    Some(name) => name.as_str().to_string(),
                    None => {
                        error!("Failed to get widget name");
                        "".to_string()
                    }
                })
                .filter(|c| c != "empty-item")
                .collect();
            if children.len() == 0 {
                empty_item.show();
            }
            if let Ok(event) = peer_receiver.lock().unwrap().try_recv() {
                let peers: CurrentPeers = match event {
                    PeerEvent::PeersUpdated(list) => list,
                    event => {
                        let _ = peer_event_sender.send(event);
                        return Continue(true);
                    }
                };
                empty_item.hide();
                for peer in peers.iter().filter(|p| !children.contains(&p.name)) {
                    let name = &peer.name;
                    let addr = &peer.address;
                    let hostname = &peer.hostname;
                    let os = &peer.os;

                    let item = PeerItem::new(name, addr, &hostname, &os);
                    let sender = file_sender.clone();
                    let item = item.bind_drag_and_drop(peer, sender);

                    layout_in.pack_start(&item.container, false, false, 0);
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

pub fn remove_expired_boxes(layout: &gtk::Box, peers: &Vec<Peer>) {
    for peer_box in layout.get_children() {
        if let Some(box_name) = peer_box.get_widget_name() {
            let box_name = box_name.as_str().to_string();
            let box_in_peers = peers
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<String>>()
                .contains(&box_name);
            if !box_in_peers && box_name != "notification" && box_name != "empty-item" {
                peer_box.destroy();
            }
        }
    }
}
