use std::env::args;
use std::error::Error;

use std::sync::{Arc, Mutex};
use std::thread;

use gio::prelude::*;
use gtk::prelude::*;
pub mod components;

use glib::Continue;
use gtk::{timeout_add, ApplicationWindow, Grid, GtkWindowExt};

use futures::channel::mpsc::{channel, Receiver, Sender};

use crate::p2p::{run_server, CurrentPeers, FileToSend, PeerEvent};
use components::{add_progress_bar, remove_expired_boxes, PeerItem, STYLE};

fn pool_peers(
    window: &ApplicationWindow,
    grid: &Grid,
    file_sender: Arc<Mutex<Sender<FileToSend>>>,
    peer_receiver: Arc<Mutex<Receiver<PeerEvent>>>,
    progress_sender: glib::Sender<Option<(usize, usize)>>,
) {
    let grid_weak = grid.downgrade();
    let weak_window = window.downgrade();

    timeout_add(200, move || {
        if let Some(hbox_in) = grid_weak.upgrade() {
            if let Ok(p) = peer_receiver.lock().unwrap().try_next() {
                let peers: CurrentPeers = match p {
                    Some(event) => match event {
                        PeerEvent::PeersUpdated(list) => list,
                        PeerEvent::TransferProgress((value, total)) => {
                            println!("Progress: {} {}", value, total);
                            let _ = progress_sender.send(Some((value, total)));
                            return Continue(true);
                        }
                    },
                    None => {
                        eprintln!("Failed to get peers from the queue");
                        return Continue(true);
                    }
                };

                let children: Vec<String> = hbox_in
                    .get_children()
                    .iter()
                    .map(|c| match c.get_widget_name() {
                        Some(name) => name.as_str().to_string(),
                        None => {
                            eprintln!("Failed to get widget name");
                            "".to_string()
                        }
                    })
                    .collect();

                let row_num: i32 = children.len() as i32 - 1;
                for peer in peers.iter().filter(|p| !children.contains(&p.name)) {
                    let name: &str = &peer.name;
                    println!("Peer: {:?}", name);

                    let item = PeerItem::new(name);
                    let sender = file_sender.clone();
                    let item = item.bind_drag_and_drop(peer, sender);

                    hbox_in.attach(&item.container, 0, row_num, 1, 1);
                }
                remove_expired_boxes(&hbox_in, &peers);
            };
        }

        if let Some(win) = weak_window.upgrade() {
            win.show_all();
        }
        Continue(true)
    });
}

pub fn build_window(
    application: &gtk::Application,
    file_sender: Arc<Mutex<Sender<FileToSend>>>,
    peer_receiver: Arc<Mutex<Receiver<PeerEvent>>>,
) -> Result<(), Box<dyn Error>> {
    glib::set_program_name(Some("Dragit"));
    let window = gtk::ApplicationWindow::new(application);

    let grid = Grid::new();
    grid.set_halign(gtk::Align::Center);

    let (progress_sender, rx) =
        glib::MainContext::channel::<Option<(usize, usize)>>(glib::PRIORITY_DEFAULT);
    let progress_bar = add_progress_bar(&grid);

    window.add(&grid);

    pool_peers(&window, &grid, file_sender, peer_receiver, progress_sender);

    rx.attach(None, move |values| match values {
        Some((v, t)) => {
            let size = v as f64;
            let total = t as f64;
            progress_bar.set_fraction(size / total);
            Continue(true)
        }
        None => Continue(false),
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

    let application =
        gtk::Application::new(Some("com.drag_and_drop2"), gio::ApplicationFlags::empty())
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
