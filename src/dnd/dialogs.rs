use gtk::prelude::*;

use bytesize::ByteSize;

use crate::p2p::TransferType;

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
    pub fn new(window: &gtk::ApplicationWindow) -> FirewallDialog {
        let dialog = gtk::MessageDialog::new(
            Some(window),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Question,
            gtk::ButtonsType::YesNo,
            concat!(
                "To work correctly, Dragit requires two open ports on the firewall.\n",
                "Your current firewall configuration would prevent the application from working.\n",
                "Would you like to let Dragit configure the firewall for you?\n",
                "If yes, you'll be prompted for password."
            ),
        );
        FirewallDialog(dialog)
    }

    pub fn run(&self) -> gtk::ResponseType {
        let resp = self.0.run();
        self.0.close();
        resp
    }
}
