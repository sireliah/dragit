use std::env::args;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

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
    AcceptFileDialog, AppNotification, NotificationType, ProgressNotification, STYLE,
};
use events::pool_peers;

pub fn build_window(
    application: &gtk::Application,
    file_sender: Arc<Mutex<Sender<FileToSend>>>,
    peer_receiver: Arc<Mutex<Receiver<PeerEvent>>>,
    command_sender: Arc<Mutex<Sender<TransferCommand>>>,
) -> Result<(), Box<dyn Error>> {
    glib::set_program_name(Some("Dragit"));
    let window = gtk::ApplicationWindow::new(application);

    let scroll = gtk::ScrolledWindow::new(gtk::NONE_ADJUSTMENT, gtk::NONE_ADJUSTMENT);
    scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);

    let overlay = gtk::Overlay::new();
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 20);
    layout.set_halign(gtk::Align::Center);
    layout.set_margin_top(60);

    let (gtk_sender, gtk_receiver) =
        glib::MainContext::channel::<PeerEvent>(glib::PRIORITY_DEFAULT);

    let alert_notif = AppNotification::new(&overlay, NotificationType::Alert);
    let error_notif = AppNotification::new(&overlay, NotificationType::Error);
    let progress = ProgressNotification::new(&overlay);

    scroll.add(&layout);
    overlay.add_overlay(&scroll);
    window.add(&overlay);

    pool_peers(&window, &layout, file_sender, peer_receiver, gtk_sender);

    let window_weak = window.downgrade();
    gtk_receiver.attach(None, move |values| match values {
        PeerEvent::TransferProgress((v, t, direction)) => {
            let size = v as f64;
            let total = t as f64;
            match direction {
                Direction::Incoming => progress.show_incoming(size, total),
                Direction::Outgoing => progress.show_outgoing(size, total),
            }
            Continue(true)
        }
        PeerEvent::FileCorrect(file_name, path) => {
            progress.progress_bar.set_fraction(0.0);
            progress.hide();
            let text = format!("Received {} \nSaved in {}", file_name, path);
            alert_notif.show(&overlay, text);
            Continue(true)
        }
        PeerEvent::FileIncorrect => {
            progress.progress_bar.set_fraction(0.0);
            progress.hide();
            error_notif.show(&overlay, "File is incorrect".to_string());
            Continue(true)
        }
        PeerEvent::FileIncoming(name, hash) => {
            if let Some(win) = window_weak.upgrade() {
                let accept_dialog = AcceptFileDialog::new(&win, name);
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
            let error = format!("Encountered an error: {:?}", error);
            error_notif.show(&overlay, error);
            Continue(true)
        }
        _ => Continue(false),
    });

    window.set_title("Dragit");
    window.set_default_size(600, 600);
    window.set_border_width(10);

    window.show_all();

    window.connect_delete_event(move |win, _| {
        win.destroy();
        Inhibit(false)
    });
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

    // TODO: remove me
    let now = SystemTime::now();
    let timestamp = now.duration_since(UNIX_EPOCH).expect("Time failed");
    let name = format!("com.drag_and_drop_{}", timestamp.as_secs());

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

// use crate::transfer::Protocol;

// fn transfer_file(protocol: impl Protocol, path: &str) -> Result<(), Box<dyn Error>> {
//     protocol.transfer_file(path)
// }

// TODO: reintegrate Bluetooth
// fn spawn_send_job(file_path: &str) -> thread::Result<()> {
//     let trimmed_path = file_path.replace("file://", "").trim().to_string();
//     let path_arc = Arc::new(trimmed_path);
//     let path_clone = Arc::clone(&path_arc);

//     thread::spawn(move || {
//         info!("Spawning thread");
//         match transfer_file(BluetoothProtocol, &path_clone) {
//             Ok(_) => (),
//             Err(err) => error!("{}", err),
//         }
//     })
//     .join()
// }

// fn push_p2p_job(file_path: String, sender: Arc<Mutex<Sender<FileToSend>>>) -> Result<(), Box<dyn Error>> {
//     let file = FileToSend::new(&file_path)?;
//     let mut sender = sender.lock().unwrap();
//     sender.send(file);

//     Ok(())
// }
