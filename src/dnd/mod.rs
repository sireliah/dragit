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

use futures::channel::mpsc::{channel, Receiver, Sender};

use crate::p2p::{run_server, FileToSend, PeerEvent};
use components::{AppNotification, ProgressNotification, STYLE};
use events::pool_peers;

pub fn build_window(
    application: &gtk::Application,
    file_sender: Arc<Mutex<Sender<FileToSend>>>,
    peer_receiver: Arc<Mutex<Receiver<PeerEvent>>>,
) -> Result<(), Box<dyn Error>> {
    glib::set_program_name(Some("Dragit"));
    let window = gtk::ApplicationWindow::new(application);

    let overlay = gtk::Overlay::new();
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 20);
    layout.set_halign(gtk::Align::Center);
    layout.set_margin_top(50);

    let (gtk_sender, rx) = glib::MainContext::channel::<PeerEvent>(glib::PRIORITY_DEFAULT);

    let app_notification = AppNotification::new(&overlay);
    let progress = ProgressNotification::new(&overlay);

    overlay.add_overlay(&layout);
    window.add(&overlay);

    pool_peers(&window, &layout, file_sender, peer_receiver, gtk_sender);

    rx.attach(None, move |values| match values {
        PeerEvent::TransferProgress((v, t)) => {
            let size = v as f64;
            let total = t as f64;
            progress.show();
            progress.progress_bar.set_fraction(size / total);
            Continue(true)
        }
        PeerEvent::FileCorrect(file_name) => {
            progress.progress_bar.set_fraction(0.0);
            progress.hide();
            app_notification.show(&overlay, file_name);
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

    // Start the p2p server in separate thread
    thread::spawn(move || match run_server(peer_sender, file_receiver) {
        Ok(_) => {}
        Err(e) => eprintln!("{:?}", e),
    });

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

        match build_window(app, file_sender_c, peer_receiver_c) {
            Ok(_) => println!("Ok!"),
            Err(e) => println!("{:?}", e),
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
//         println!("Spawning thread");
//         match transfer_file(BluetoothProtocol, &path_clone) {
//             Ok(_) => (),
//             Err(err) => eprintln!("{}", err),
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
