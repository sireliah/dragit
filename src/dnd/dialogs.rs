use gtk::prelude::*;

use bytesize::ByteSize;

use crate::p2p::TransferType;
use crate::user_data::UserConfig;

pub struct AcceptFileDialog(gtk::MessageDialog);

impl AcceptFileDialog {
    pub fn new(
        window: &gtk::ApplicationWindow,
        name: String,
        size: usize,
        transfer_type: TransferType,
    ) -> AcceptFileDialog {
        let readable_size = ByteSize(size as u64);
        let message = match transfer_type {
            TransferType::File => format!(
                "Incoming file {} ({}).\n\nWould you like to accept?",
                name, readable_size
            ),
            TransferType::Text => format!("Incoming text {}.\n\nWould you like to accept?", name),
        };
        let dialog = gtk::MessageDialog::new(
            Some(window),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Question,
            gtk::ButtonsType::YesNo,
            &message,
        );
        AcceptFileDialog(dialog)
    }

    pub fn run(&self) -> gtk::ResponseType {
        let resp = self.0.run();
        self.0.close();
        resp
    }
}

pub struct FirewallDialog(gtk::MessageDialog);

impl FirewallDialog {
    pub fn new(window: &gtk::ApplicationWindow, config: &UserConfig) -> FirewallDialog {
        let port = config.get_port();
        let message = concat!(
            "Your current firewall configuration prevents Dragit from working.\n",
            "\n",
            "Dragit can configure the firewall for you. Would you like it to do so?\n",
            "If yes, you'll be prompted for password.\n",
            "\n",
            "Following ports will be added:\n",
            "- tcp 5353\n",
            "- tcp ",
        );
        let text = format!("{}{}", message, port);
        let dialog = gtk::MessageDialog::new(
            Some(window),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Question,
            gtk::ButtonsType::YesNo,
            &text,
        );
        FirewallDialog(dialog)
    }

    pub fn run(&self) -> gtk::ResponseType {
        let resp = self.0.run();
        self.0.close();
        resp
    }
}
