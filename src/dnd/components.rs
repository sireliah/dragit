use std::error::Error;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::Sender;

use gdk::DragAction;
use gtk::prelude::*;
use gtk::{DestDefaults, Label, TargetEntry, TargetFlags};
use percent_encoding::percent_decode_str;

use crate::p2p::{FileToSend, Peer};

#[derive(Debug)]
pub struct PeerItem {
    pub label: Label,
}

impl PeerItem {
    pub fn new(name: &str) -> PeerItem {
        PeerItem {
            label: Label::new(Some(name)),
        }
    }

    pub fn bind_drag_and_drop(
        self,
        peer: &Peer,
        file_sender: Arc<Mutex<Sender<FileToSend>>>,
    ) -> Self {
        let peer_id = peer.peer_id.clone();
        let targets = vec![
            TargetEntry::new("STRING", TargetFlags::OTHER_APP, 0),
            TargetEntry::new("text/uri-list", TargetFlags::OTHER_APP, 0),
        ];
        self.label
            .drag_dest_set(DestDefaults::ALL, &targets, DragAction::COPY);

        self.label
            .connect_drag_data_received(move |_win, _, _, _, s, _, _| {
                let path: String = match s.get_text() {
                    Some(value) => PeerItem::clean_filename(&value).expect("Decoding path failed"),
                    None => s.get_uris().pop().unwrap().to_string(),
                };
                let file = match FileToSend::new(&path, &peer_id) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("Failed creating FileToSend {:?}", e);
                        return ();
                    }
                };
                let mut sender = file_sender.lock().unwrap();
                sender.try_send(file).expect("Sending failed");
            });

        self.label.connect_drag_motion(|w, _, _, _, _| {
            match w.get_text() {
                Some(value) => {
                    let filename = PeerItem::clean_filename(&value).expect("Decoding path failed");
                    w.set_text(&filename);
                }
                None => w.set_text("[FILE]>"),
            };
            gtk::Inhibit(false)
        });

        self
    }

    fn clean_filename(path: &str) -> Result<String, Box<dyn Error>> {
        let value = percent_decode_str(path).decode_utf8()?;
        let path = value.replace("file://", "");
        Ok(path.trim().to_string())
    }
}

pub fn remove_expired_boxes(hbox_in: &gtk::Box, peers: &Vec<Peer>) {
    for peer_box in hbox_in.get_children() {
        if let Some(box_name) = peer_box.get_widget_name() {
            let box_name = box_name.as_str().to_string();
            let box_in_peers = peers
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<String>>()
                .contains(&box_name);
            if !box_in_peers {
                hbox_in.remove(&peer_box);
                peer_box.destroy();
            }
        }
    }
}
