use std::sync::{Arc, Mutex};

use gio::prelude::*;
use gtk::prelude::*;

use async_std::sync::{Receiver, Sender};

use glib::Continue;
use gtk::{timeout_add, ApplicationWindow};

use crate::dnd::components::{EmptyListItem, PeerItem};
use crate::p2p::{CurrentPeers, FileToSend, PeerEvent};

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
                .map(|c| {
                    c.get_widget_name()
                        .unwrap_or(glib::GString::from(""))
                        .to_string()
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

                // Clear the item list before receiving new list of peers from event
                remove_items(&layout_in);

                for peer in peers.iter() {
                    let name = &peer.name;
                    let addr = &peer.address;
                    let hostname = &peer.hostname;
                    let os = &peer.os;

                    let item = PeerItem::new(name, addr, &hostname, &os);
                    let sender = file_sender.clone();
                    let item = item.bind_drag_and_drop(peer, sender);

                    layout_in.pack_start(&item.container, false, false, 0);
                }
            };
        }

        if let Some(win) = weak_window.upgrade() {
            win.show_all();
        }
        Continue(true)
    });
}

fn remove_items(layout: &gtk::Box) {
    for child in layout.get_children().iter().filter(|c| {
        let name = c
            .get_widget_name()
            .unwrap_or(glib::GString::from(""))
            .to_string();
        name != "notification" && name != "empty-item"
    }) {
        layout.remove(child);
        child.destroy();
    }
}
