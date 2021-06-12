use gtk::prelude::*;
use gtk::Label;

use crate::dnd::components::get_link;
use crate::p2p::Payload;

pub struct ProgressNotification {
    revealer: gtk::Revealer,
    overlay: gtk::Overlay,
    pub progress_bar: gtk::ProgressBar,
}

impl ProgressNotification {
    pub fn new(main_overlay: &gtk::Overlay) -> Self {
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        layout.set_widget_name("notification");

        let overlay = gtk::Overlay::new();
        let revealer = gtk::Revealer::new();
        let progress_bar = gtk::ProgressBar::new();

        revealer.set_halign(gtk::Align::Center);
        revealer.set_valign(gtk::Align::Start);
        revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);

        progress_bar.set_text(Some("Receiving file"));
        progress_bar.set_show_text(true);

        progress_bar.set_halign(gtk::Align::Center);
        progress_bar.set_valign(gtk::Align::Start);
        progress_bar.set_hexpand(true);
        progress_bar.set_size_request(500, 50);
        revealer.set_margin_bottom(30);

        layout.pack_start(&progress_bar, true, false, 0);
        revealer.add(&layout);

        overlay.add_overlay(&revealer);

        main_overlay.add_overlay(&overlay);
        revealer.set_reveal_child(false);

        ProgressNotification {
            revealer,
            overlay,
            progress_bar,
        }
    }

    fn show(&self, main_overlay: &gtk::Overlay) {
        main_overlay.reorder_overlay(&self.overlay, 10);
        self.revealer.set_reveal_child(true)
    }

    fn show_progress(&self, main_overlay: &gtk::Overlay, size: f64, total: f64, text: &str) {
        self.show(main_overlay);
        self.progress_bar.set_fraction(size / total);
        self.progress_bar.set_text(Some(text));
    }

    pub fn show_incoming(&self, main_overlay: &gtk::Overlay, size: f64, total: f64) {
        self.show_progress(main_overlay, size, total, "Receiving file");
    }

    pub fn show_outgoing(&self, main_overlay: &gtk::Overlay, size: f64, total: f64) {
        self.show_progress(main_overlay, size, total, "Sending file");
    }

    pub fn hide(&self, main_overlay: &gtk::Overlay) {
        main_overlay.reorder_overlay(&self.overlay, 0);

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
    layout: gtk::Grid,
    link_pos: i32,
}

impl AppNotification {
    pub fn new(main_overlay: &gtk::Overlay, notification_type: NotificationType) -> Self {
        let layout = gtk::Grid::new();
        let overlay = gtk::Overlay::new();
        let revealer = gtk::Revealer::new();
        let label = Label::new(Some("File correct"));
        label.set_halign(gtk::Align::Start);

        let button_close =
            gtk::Button::from_icon_name(Some("window-close-symbolic"), gtk::IconSize::SmallToolbar);
        button_close.set_widget_name("button-close");
        button_close.set_relief(gtk::ReliefStyle::None);
        button_close.set_size_request(40, 40);

        let revealer_weak = revealer.downgrade();
        let main_overlay_weak = main_overlay.downgrade();
        let overlay_weak = overlay.downgrade();

        button_close.connect_clicked(move |_| {
            if let (Some(rev), Some(over), Some(o)) = (
                revealer_weak.upgrade(),
                main_overlay_weak.upgrade(),
                overlay_weak.upgrade(),
            ) {
                rev.set_reveal_child(false);
                over.reorder_overlay(&o, 0);
            }
        });

        revealer.set_halign(gtk::Align::Center);
        revealer.set_valign(gtk::Align::Start);
        revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);

        let icon = AppNotification::set_icon(notification_type);
        icon.set_size_request(50, 40);

        layout.set_widget_name("notification");
        layout.attach(&icon, 0, 0, 1, 1);
        layout.attach(&label, 1, 0, 1, 1);
        layout.attach(&button_close, 4, 0, 1, 1);

        revealer.add(&layout);
        overlay.add_overlay(&revealer);

        main_overlay.add_overlay(&overlay);
        revealer.set_reveal_child(false);

        AppNotification {
            revealer,
            overlay,
            label,
            layout,
            link_pos: 2,
        }
    }

    fn set_icon(notification_type: NotificationType) -> gtk::Image {
        match notification_type {
            NotificationType::Alert => {
                gtk::Image::from_icon_name(Some("dialog-information"), gtk::IconSize::Button)
            }
            NotificationType::Error => {
                gtk::Image::from_icon_name(Some("dialog-warning"), gtk::IconSize::Button)
            }
        }
    }

    fn reveal(&self, overlay: &gtk::Overlay) {
        overlay.reorder_overlay(&self.overlay, 10);
        self.revealer.set_reveal_child(true);
    }

    pub fn hide(&self, main_overlay: &gtk::Overlay) {
        main_overlay.reorder_overlay(&self.overlay, 0);

        self.revealer.set_reveal_child(false)
    }

    pub fn show_text(&self, overlay: &gtk::Overlay, text: &str) {
        self.remove_link();
        self.label.set_text(text);
        self.reveal(overlay);
    }

    fn remove_link(&self) {
        if let Some(child) = self.layout.get_child_at(self.link_pos, 0) {
            self.layout.remove(&child);
        };
    }

    pub fn show_payload(&self, overlay: &gtk::Overlay, file_name: &str, payload: &Payload) {
        match payload {
            Payload::Path(path) => {
                self.label.set_text("Received");
                self.remove_link();
                let link = get_link(file_name, &path);
                self.layout.attach(&link, self.link_pos, 0, 1, 1);
            }
            Payload::Text(_) => {
                self.remove_link();
                self.label.set_text("Received text");
            }
        };

        self.reveal(overlay);
    }
}
