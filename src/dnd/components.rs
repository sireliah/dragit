use std::error::Error;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::Sender;

use gdk::DragAction;
use gtk::prelude::*;
use gtk::{DestDefaults, Label, TargetEntry, TargetFlags};
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
}
#notification-alert {
    padding: 10px;
    border-radius: 10px;
    background-color: rgb(100, 100, 100);
}
#button-close {
    padding: 0;
    margin: 0;
    border: none;
    border-radius: 10px;
}
#button-close:hover {
    background-image: none;
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

pub fn remove_expired_boxes(layout: &gtk::Box, peers: &Vec<Peer>) {
    for peer_box in layout.get_children() {
        if let Some(box_name) = peer_box.get_widget_name() {
            let box_name = box_name.as_str().to_string();
            let box_in_peers = peers
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<String>>()
                .contains(&box_name);
            if !box_in_peers && box_name != "notification" {
                peer_box.destroy();
            }
        }
    }
}

pub struct ProgressNotification {
    revealer: gtk::Revealer,
    pub progress_bar: gtk::ProgressBar,
}

impl ProgressNotification {
    pub fn new(overlay: &gtk::Overlay) -> Self {
        let revealer = gtk::Revealer::new();
        let ov = gtk::Overlay::new();
        let progress_bar = gtk::ProgressBar::new();
        revealer.set_halign(gtk::Align::Center);
        revealer.set_valign(gtk::Align::Start);

        revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);

        progress_bar.set_halign(gtk::Align::Center);
        progress_bar.set_valign(gtk::Align::Start);
        progress_bar.set_text(Some("Receiving file"));
        progress_bar.set_show_text(true);
        progress_bar.set_hexpand(true);
        progress_bar.set_size_request(500, 50);
        revealer.set_margin_bottom(30);

        ov.add_overlay(&revealer);
        revealer.add(&progress_bar);
        revealer.set_widget_name("notification");
        overlay.add_overlay(&ov);
        revealer.set_reveal_child(false);

        ProgressNotification {
            revealer,
            progress_bar,
        }
    }

    pub fn show(&self) {
        self.revealer.set_reveal_child(true)
    }

    pub fn hide(&self) {
        self.revealer.set_reveal_child(false)
    }
}

pub struct AppNotification {
    revealer: gtk::Revealer,
    pub overlay: gtk::Overlay,
    label: Label,
}

impl AppNotification {
    pub fn new(main_overlay: &gtk::Overlay) -> Self {
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        let overlay = gtk::Overlay::new();
        let revealer = gtk::Revealer::new();
        let label = Label::new(Some("File correct"));
        let button_close = gtk::Button::new_from_icon_name(
            Some("window-close-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        button_close.set_widget_name("button-close");
        button_close.set_relief(gtk::ReliefStyle::None);

        let revealer_weak = revealer.downgrade();
        let main_overlay_weak = main_overlay.downgrade();
        let overlay_weak = overlay.downgrade();

        button_close.connect_clicked(move |_| {
            if let (Some(r), Some(mo), Some(o)) = (
                revealer_weak.upgrade(),
                main_overlay_weak.upgrade(),
                overlay_weak.upgrade(),
            ) {
                r.set_reveal_child(false);
                mo.reorder_overlay(&o, 0);
            }
        });

        revealer.set_halign(gtk::Align::Center);
        revealer.set_valign(gtk::Align::Start);

        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::Center);
        label.set_size_request(400, 50);

        revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);

        layout.pack_start(&label, true, false, 0);
        layout.pack_start(&button_close, true, false, 0);
        layout.set_widget_name("notification-alert");

        revealer.add(&layout);
        overlay.add_overlay(&revealer);

        main_overlay.add_overlay(&overlay);
        revealer.set_reveal_child(false);

        AppNotification {
            revealer,
            overlay,
            label,
        }
    }

    pub fn show(&self, overlay: &gtk::Overlay, text: String) {
        overlay.reorder_overlay(&self.overlay, 10);
        let notif_text = format!("File received {}", text);
        self.label.set_text(&notif_text);
        self.revealer.set_reveal_child(true);
    }
}
