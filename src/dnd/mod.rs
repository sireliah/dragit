extern crate cairo;
extern crate gdk;
extern crate gio;
extern crate glib;
extern crate gtk;

use futures::channel::mpsc::{channel, Receiver, Sender};
use percent_encoding::percent_decode_str;

use std::env::args;
use std::error::Error;

use std::sync::{Arc, Mutex};
use std::thread;

use self::gio::prelude::*;
use self::gtk::prelude::*;

// use self::gdk::ScreenExt;
use self::glib::Continue;
use self::gtk::GtkWindowExt;
use self::gtk::{timeout_add, ApplicationWindow};

use crate::p2p::{run_server, FileToSend, Peer};
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

fn clean_filename(path: &str) -> Result<String, Box<dyn Error>> {
    let value = percent_decode_str(path).decode_utf8()?;
    let path = value.replace("file://", "");
    Ok(path.trim().to_string())
}

fn remove_expired_boxes(hbox_in: &gtk::Box, peers: &Vec<Peer>) {
    for peer_box in hbox_in.get_children() {
        if let Some(box_name) = peer_box.get_widget_name() {
            let box_name = box_name.as_str().to_string();
            let box_in_peers = peers
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<String>>()
                .contains(&box_name);
            if !box_in_peers {
                hbox_in.remove(&peer_box);
                peer_box.destroy();
            }
        }
    }
}

fn pool_peers(
    window: &gtk::ApplicationWindow,
    hbox: &gtk::Box,
    file_sender: Arc<Mutex<Sender<FileToSend>>>,
    peer_receiver: Arc<Mutex<Receiver<Vec<Peer>>>>,
) {
    let hbox_weak = hbox.downgrade();
    let weak_window = window.downgrade();
    let targets = vec![
        gtk::TargetEntry::new("STRING", gtk::TargetFlags::OTHER_APP, 0),
        gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::OTHER_APP, 0),
    ];

    timeout_add(100, move || {
        if let Some(hbox_in) = hbox_weak.upgrade() {
            if let Ok(p) = peer_receiver.lock().unwrap().try_next() {
                let peers: Vec<Peer> = p.unwrap();

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
                println!("{:?}", peers);
                println!("Children: {:?}", children);

                for p in peers.iter().filter(|p| !children.contains(&p.name)) {
                    let name: &str = &p.name;
                    let peer_id = p.peer_id.clone();
                    println!("Peer: {:?}", name);

                    let box_row = gtk::Box::new(gtk::Orientation::Vertical, 10);
                    let new_label = gtk::Label::new(Some(name));

                    box_row.pack_start(&new_label, false, false, 20);

                    let fs = file_sender.clone();
                    new_label.drag_dest_set(
                        gtk::DestDefaults::ALL,
                        &targets,
                        gdk::DragAction::COPY,
                    );

                    new_label.connect_drag_data_received(move |_win, _, _, _, s, _, _| {
                        let path: String = match s.get_text() {
                            Some(value) => clean_filename(&value).expect("Decoding path failed"),
                            None => s.get_uris().pop().unwrap().to_string(),
                        };
                        let file = match FileToSend::new(&path, &peer_id) {
                            Ok(v) => v,
                            Err(e) => {
                                eprintln!("Failed creating FileToSend {:?}", e);
                                return ();
                            }
                        };
                        let mut sender = fs.lock().unwrap();
                        sender.try_send(file).expect("Sending failed");
                    });

                    new_label.connect_drag_motion(|w, _, _, _, _| {
                        match w.get_text() {
                            Some(value) => {
                                let filename =
                                    clean_filename(&value).expect("Decoding path failed");
                                w.set_text(&filename);
                            }
                            None => w.set_text("[FILE]>"),
                        };
                        gtk::Inhibit(false)
                    });

                    box_row.set_widget_name(name);
                    hbox_in.pack_start(&box_row, false, false, 20);
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
    peer_receiver: Arc<Mutex<Receiver<Vec<Peer>>>>,
) -> Result<(), Box<dyn Error>> {
    let window = gtk::ApplicationWindow::new(application);

    let hbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
    window.add(&hbox);

    pool_peers(&window, &hbox, file_sender, peer_receiver);

    set_visual(&window, &None);
    window.connect_draw(draw);

    window.set_title("Dragit!");
    window.set_default_size(600, 600);
    window.set_app_paintable(true);

    // Those will set transparent bar on left edge of the screen
    // window.set_default_size(5, 1000);
    // window.set_decorated(false);
    // window.set_skip_taskbar_hint(true);
    // window.move_(0, 0);
    // window.set_keep_above(true);

    window.show_all();

    window.connect_delete_event(move |win, _| {
        win.destroy();
        Inhibit(false)
    });
    Ok(())
}

fn set_visual(window: &ApplicationWindow, _screen: &Option<gdk::Screen>) {
    if let Some(screen) = window.get_screen() {
        if let Some(visual) = screen.get_rgba_visual() {
            window.set_visual(Some(&visual));
        }
    }
}

fn draw(_window: &ApplicationWindow, ctx: &cairo::Context) -> Inhibit {
    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.9);
    ctx.set_operator(cairo::Operator::Screen);
    ctx.paint();
    Inhibit(false)
}

pub fn start_window() {
    let (file_sender, file_receiver) = channel::<FileToSend>(1024 * 24);
    let (peer_sender, peer_receiver) = channel::<Vec<Peer>>(1024 * 24);

    // Start the p2p server in separate thread
    thread::spawn(move || match run_server(peer_sender, file_receiver) {
        Ok(_) => {}
        Err(e) => eprintln!("{:?}", e),
    });

    let peer_receiver_arc = Arc::new(Mutex::new(peer_receiver));

    let application =
        gtk::Application::new(Some("com.drag_and_drop"), gio::ApplicationFlags::empty())
            .expect("Initialization failed...");

    application.connect_startup(move |app| {
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
