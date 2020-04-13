use std::error::Error;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::Sender;

use gdk::DragAction;
use gtk::prelude::*;
use gtk::{DestDefaults, Grid, Label, TargetEntry, TargetFlags};
use percent_encoding::percent_decode_str;

use crate::p2p::{FileToSend, Peer};

pub const STYLE: &str = "
#drop-label {
    padding: 10px;
    margin: 10px;
    border: 1px;
    border-style: dashed;
    border-radius: 15px;
    background-color: rgb(240, 240, 240); 
}";

#[derive(Debug)]
pub struct PeerItem {
    pub container: gtk::Box,
    pub label: Label,
    pub progress: Option<gtk::ProgressBar>,
}

impl PeerItem {
    pub fn new(name: &str) -> PeerItem {
        let label = Label::new(Some(name));
        label.set_widget_name("drop-label");
        label.set_halign(gtk::Align::Center);
        label.set_size_request(500, 100);

        let container = gtk::Box::new(gtk::Orientation::Vertical, 10);
        container.set_widget_name(name);
        container.pack_start(&label, false, false, 10);

        PeerItem {
            container,
            label,
            progress: None,
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
            // TODO: use different content type here
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

        self.label.connect_drag_motion(|w, _context, _, _, _| {
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

pub fn remove_expired_boxes(grid: &Grid, peers: &Vec<Peer>) {
    for peer_box in grid.get_children() {
        if let Some(box_name) = peer_box.get_widget_name() {
            let box_name = box_name.as_str().to_string();
            let box_in_peers = peers
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<String>>()
                .contains(&box_name);
            if !box_in_peers && box_name != "bar" {
                peer_box.destroy();
            }
        }
    }
}

pub fn add_progress_bar(grid: &gtk::Grid) -> gtk::ProgressBar {
    let progress = gtk::ProgressBar::new();
    progress.set_text(Some("Receiving"));
    progress.set_show_text(true);
    progress.set_hexpand(true);
    progress.set_widget_name("bar");
    grid.attach(&progress, 0, 3, 1, 1);
    progress
}
