use std::error::Error;
use std::sync::{Arc, Mutex};

use async_std::sync::Sender;

use gdk::DragAction;
use gio;
use gtk::prelude::*;
use gtk::{DestDefaults, Label, TargetEntry, TargetFlags};

use libp2p::{multiaddr::Protocol, Multiaddr};
use percent_encoding::percent_decode_str;

use crate::p2p::{FileToSend, Peer};

pub const STYLE: &str = "
#drop-zone {
    padding: 10px;
    margin: 10px;
    border: 1px;
    border-style: dashed;
    border-radius: 15px;
}
#notification-alert {
    padding: 10px;
    border-radius: 10px;
    color: rgb(0, 0, 0);
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
}

impl PeerItem {
    pub fn new(name: &str, address: &Multiaddr) -> PeerItem {
        let ip = PeerItem::extract_ip(&address);
        let display_name = format!("<big><b>{}</b></big> \n<small>{}</small>", ip, name);

        let label = Label::new(None);
        label.set_markup(&display_name);
        label.set_widget_name("drop-label");
        label.set_halign(gtk::Align::Center);
        label.set_size_request(500, 100);

        let image = gtk::Image::new_from_icon_name(Some("insert-object"), gtk::IconSize::Dialog);

        let container = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let inner_container = gtk::Box::new(gtk::Orientation::Vertical, 10);

        container.set_widget_name(name);
        inner_container.set_widget_name("drop-zone");

        inner_container.pack_start(&image, false, false, 10);
        inner_container.pack_start(&label, false, false, 10);
        container.pack_start(&inner_container, false, false, 0);

        PeerItem { container, label }
    }

    fn extract_ip(address: &Multiaddr) -> String {
        let components = address.iter().collect::<Vec<Protocol>>();
        let ip = &components[0];
        ip.to_string().replace("/ip4/", "").replace("/ip6/", "")
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
        self.container
            .drag_dest_set(DestDefaults::ALL, &targets, DragAction::COPY);

        self.container
            .connect_drag_data_received(move |_win, _, _, _, s, _, _| {
                let path: String = match s.get_text() {
                    Some(value) => PeerItem::clean_filename(&value).expect("Decoding path failed"),
                    None => {
                        // Extracting the file path from the URI works best for Windows
                        let uri = s.get_uris().pop().unwrap().to_string();
                        PeerItem::clean_filename(&uri).expect("Decoding path from URI failed")
                    }
                };
                let file = match FileToSend::new(&path, &peer_id) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("Failed creating FileToSend {:?}", e);
                        return ();
                    }
                };
                let sender = file_sender.lock().unwrap();
                sender.try_send(file).expect("Sending failed");
            });

        self
    }

    fn clean_filename(path: &str) -> Result<String, Box<dyn Error>> {
        let value = percent_decode_str(path).decode_utf8()?;

        let path = clean_file_proto(&value);
        Ok(path.trim().to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn clean_file_proto(value: &str) -> String {
    value.replace("file://", "")
}

#[cfg(target_os = "windows")]
fn clean_file_proto(value: &str) -> String {
    // Windows paths contain one extra slash
    value.replace("file:///", "")
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

pub enum NotificationType {
    Alert,
    Error,
}

pub struct AppNotification {
    revealer: gtk::Revealer,
    pub overlay: gtk::Overlay,
    label: Label,
}

impl AppNotification {
    pub fn new(main_overlay: &gtk::Overlay, notification_type: NotificationType) -> Self {
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        let overlay = gtk::Overlay::new();
        let revealer = gtk::Revealer::new();
        let label = Label::new(Some("File correct"));

        layout.set_widget_name("notification-alert");

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

        let icon = AppNotification::set_icon(notification_type);

        layout.pack_start(&icon, true, false, 0);
        layout.pack_start(&label, true, false, 0);
        layout.pack_start(&button_close, true, false, 0);

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

    fn set_icon(notification_type: NotificationType) -> gtk::Image {
        match notification_type {
            NotificationType::Alert => {
                let ico = gio::Icon::new_for_string("dialog-information").unwrap();
                gtk::Image::new_from_gicon(&ico, gtk::IconSize::Button)
            }
            NotificationType::Error => {
                let ico = gio::Icon::new_for_string("dialog-warning").unwrap();
                gtk::Image::new_from_gicon(&ico, gtk::IconSize::Button)
            }
        }
    }

    fn reveal(&self, overlay: &gtk::Overlay) {
        overlay.reorder_overlay(&self.overlay, 10);
        self.revealer.set_reveal_child(true);
    }

    pub fn show(&self, overlay: &gtk::Overlay, text: String) {
        self.label.set_text(&text);

        self.reveal(overlay);
    }
}

pub struct AcceptFileDialog(gtk::MessageDialog);

impl AcceptFileDialog {
    pub fn new(window: &gtk::ApplicationWindow, name: String) -> AcceptFileDialog {
        let dialog = gtk::MessageDialog::new(
            Some(window),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Question,
            gtk::ButtonsType::YesNo,
            &format!("Incoming file {}. Would you like to accept it?", name),
        );
        AcceptFileDialog(dialog)
    }

    pub fn run(&self) -> gtk::ResponseType {
        let resp = self.0.run();
        self.0.destroy();
        resp
    }
}
