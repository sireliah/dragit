use std::sync::{Arc, Mutex};
use std::time::Duration;

use gio::prelude::*;
use gtk::prelude::*;

use async_std::channel::{Receiver, Sender};

use glib::{timeout_add_local, Continue};
use gtk::ApplicationWindow;

use crate::dnd::components::{get_item_name, EmptyListItem, PeerItem};
use crate::p2p::{CurrentPeers, FileToSend, PeerEvent};

pub fn pool_peers(
    window: &ApplicationWindow,
    layout: &gtk::ListBox,
    file_sender: Arc<Mutex<Sender<FileToSend>>>,
    peer_receiver: Arc<Mutex<Receiver<PeerEvent>>>,
    peer_event_sender: glib::Sender<PeerEvent>,
) {
    // TODO: investigate why set_placeholder() doesn't work
    let empty_item = EmptyListItem::new();
    layout.add(&empty_item.container);
    empty_item.show();

    let layout_weak = layout.downgrade();
    let weak_window = window.downgrade();

    let interval = Duration::from_millis(200);
    timeout_add_local(interval, move || {
        if let Some(layout_in) = layout_weak.upgrade() {
            let children: Vec<String> = layout_in
                .children()
                .iter()
                .map(|c| get_item_name(c))
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

                    layout_in.add(&item.container);
                }
            };
        }

        if let Some(win) = weak_window.upgrade() {
            win.show_all();
        }
        Continue(true)
    });
}

fn remove_items(layout: &gtk::ListBox) {
    for child in layout.children().iter().filter(|c| {
        let name = get_item_name(*c);
        name != "notification" && name != "empty-item"
    }) {
        layout.remove(child);
    }
}
