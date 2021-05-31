use std::env::args;
use std::error::Error;

use std::sync::{Arc, Mutex};
use std::thread;

use gio::prelude::*;
use gtk::prelude::*;
pub mod components;
mod events;

use glib::Continue;
use gtk::GtkWindowExt;

use async_std::sync::{channel, Receiver, Sender};

use crate::p2p::{peer::Direction, run_server, FileToSend, PeerEvent, TransferCommand};
use components::{
    AcceptFileDialog, AppNotification, MainLayout, NotificationType, ProgressNotification, STYLE,
};
use events::pool_peers;

pub fn build_window(
    application: &gtk::Application,
    file_sender: Arc<Mutex<Sender<FileToSend>>>,
    peer_receiver: Arc<Mutex<Receiver<PeerEvent>>>,
    command_sender: Arc<Mutex<Sender<TransferCommand>>>,
) -> Result<(), Box<dyn Error>> {
    let title = format!("Dragit {}", env!("CARGO_PKG_VERSION"));

    glib::set_program_name(Some(&title));
    let window = gtk::ApplicationWindow::new(application);

    let layout = MainLayout::new()?;

    let overlay = gtk::Overlay::new();
    window.set_titlebar(Some(&layout.bar));

    let (gtk_sender, gtk_receiver) =
        glib::MainContext::channel::<PeerEvent>(glib::PRIORITY_DEFAULT);

    let alert_notif = AppNotification::new(&overlay, NotificationType::Alert);
    let error_notif = AppNotification::new(&overlay, NotificationType::Error);
    let progress = ProgressNotification::new(&overlay);

    overlay.add_overlay(&layout.layout);

    // Application window has overlay on the top, so we can show notifications on it
    window.add(&overlay);

    pool_peers(
        &window,
        &layout.item_layout,
        file_sender,
        peer_receiver,
        gtk_sender,
    );

    let window_weak = window.downgrade();
    gtk_receiver.attach(None, move |values| match values {
        PeerEvent::TransferProgress((v, t, direction)) => {
            alert_notif.hide(&overlay);
            let size = v as f64;
            let total = t as f64;
            match direction {
                Direction::Incoming => progress.show_incoming(&overlay, size, total),
                Direction::Outgoing => progress.show_outgoing(&overlay, size, total),
            }
            Continue(true)
        }
        PeerEvent::WaitingForAnswer => {
            alert_notif.show_text(&overlay, "Waiting for answer from the other device...");
            Continue(true)
        }
        PeerEvent::TransferCompleted => {
            progress.hide(&overlay);
            Continue(true)
        }
        PeerEvent::FileCorrect(file_name, payload) => {
            progress.progress_bar.set_fraction(0.0);
            progress.hide(&overlay);

            alert_notif.show_payload(&overlay, &file_name, &payload);
            layout.add_recent_file(&file_name, payload);

            Continue(true)
        }
        PeerEvent::FileIncorrect => {
            progress.progress_bar.set_fraction(0.0);
            progress.hide(&overlay);
            error_notif.show_text(&overlay, "File is incorrect");
            Continue(true)
        }
        PeerEvent::FileIncoming(name, hash, size, transfer_type) => {
            if let Some(win) = window_weak.upgrade() {
                let accept_dialog = AcceptFileDialog::new(&win, name, size, transfer_type);
                let response = accept_dialog.run();

                let command = match response {
                    gtk::ResponseType::Yes => TransferCommand::Accept(hash),
                    gtk::ResponseType::No => TransferCommand::Deny(hash),
                    _ => TransferCommand::Deny(hash),
                };

                let _ = command_sender.lock().unwrap().try_send(command);
            }
            Continue(true)
        }
        PeerEvent::Error(error) => {
            error!("Got error: {}", error);
            progress.hide(&overlay);

            let error = format!("Encountered an error: {:?}", error);
            error_notif.show_text(&overlay, &error);
            Continue(true)
        }
        _ => Continue(false),
    });

    window.set_default_size(600, 750);
    window.set_border_width(10);

    window.show_all();

    window.connect_delete_event(move |_win, _| Inhibit(false));
    Ok(())
}

pub fn start_window() {
    let (file_sender, file_receiver) = channel::<FileToSend>(1024 * 24);
    let (peer_sender, peer_receiver) = channel::<PeerEvent>(1024 * 24);
    let (command_sender, command_receiver) = channel::<TransferCommand>(1024 * 24);

    // Start the p2p server in separate thread
    let sender_clone = peer_sender.clone();
    thread::spawn(
        move || match run_server(peer_sender, file_receiver, command_receiver) {
            Ok(_) => {}
            Err(e) => {
                error!("Server error: {:?}", e);
                let _ = sender_clone
                    .try_send(PeerEvent::Error(e.to_string()))
                    .unwrap();
            }
        },
    );

    let peer_receiver_arc = Arc::new(Mutex::new(peer_receiver));

    let name = "com.sireliah.Dragit";

    info!("Starting {}", name);

    let application = gtk::Application::new(Some(&name), gio::ApplicationFlags::empty())
        .expect("Initialization failed...");

    application.connect_startup(move |app| {
        let provider = gtk::CssProvider::new();
        provider
            .load_from_data(STYLE.as_bytes())
            .expect("Failed to load CSS");
        gtk::StyleContext::add_provider_for_screen(
            &gdk::Screen::get_default().expect("Error initializing gtk css provider."),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let file_sender_c = Arc::new(Mutex::new(file_sender.clone()));
        let peer_receiver_c = Arc::clone(&peer_receiver_arc);
        let command_sender_c = Arc::new(Mutex::new(command_sender.clone()));

        match build_window(app, file_sender_c, peer_receiver_c, command_sender_c) {
            Ok(_) => info!("Window started"),
            Err(e) => error!("Window error: {:?}", e),
        };
    });
    application.connect_activate(|_| {});

    application.run(&args().collect::<Vec<_>>());
}
