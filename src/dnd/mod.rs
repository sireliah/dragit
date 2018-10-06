extern crate cairo;
extern crate gdk;
extern crate gio;
extern crate gtk;

use std::env::args;
use std::error::Error;
use std::sync::Arc;
use std::thread;

use self::gio::prelude::*;
use self::gtk::prelude::*;

use self::gtk::ApplicationWindow;

use self::gdk::ScreenExt;

use bluetooth;
use std::path::Path;


fn spawn_send_job(file_path: &str) -> thread::Result<()> {
    let trimmed_path = file_path.replace("file://", "").trim().to_string();
    let path_arc = Arc::new(trimmed_path);
    let path_clone = Arc::clone(&path_arc);

    thread::spawn(move || {
        println!("Spawning thread");
        match bluetooth::transfer_file(&path_clone) {
            Ok(_) => (),
            Err(err) => println!("{}", err),
        }
    }).join()
}

pub fn build_window(application: &gtk::Application) -> Result<(), Box<Error>> {
    let targets = vec![
        gtk::TargetEntry::new("STRING", gtk::TargetFlags::OTHER_APP, 0),
        gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::OTHER_APP, 0),
    ];
    let label = gtk::Label::new("D");
    label.drag_dest_set(gtk::DestDefaults::ALL, &targets, gdk::DragAction::COPY);

    label.connect_drag_motion(|w, _, _, _, _| {
        w.set_text("New file");
        gtk::Inhibit(false)
    });

    label.connect_drag_data_received(|w, _, _, _, s, _, _| {
        let path: String = match s.get_text() {
            Some(value) => value,
            None => s.get_uris().pop().unwrap(),
        };

        if let Some(file_path) = Path::new(&path).to_str() {
            match spawn_send_job(&file_path) {
                Ok(_) => println!("Thread finished."),
                Err(_) => println!("Thread panicked!"),
            }
        } else {
            println!("Problem with the file path");
        }
        w.set_text("D");
    });

    // Stack the button and label horizontally
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox.pack_start(&label, true, true, 0);

    // Finish populating the window and display everything
    let window = gtk::ApplicationWindow::new(application);

    set_visual(&window, &None);
    window.connect_screen_changed(set_visual);
    window.connect_draw(draw);

    window.set_title("Dragit!");
    window.set_default_size(5, 1000);
    window.add(&hbox);
    window.set_app_paintable(true);
    window.set_decorated(false);
    window.set_skip_taskbar_hint(true);
    window.move_(0, 0);
    window.set_keep_above(true);
    window.show_all();

    // GTK & main window boilerplate
    window.connect_delete_event(move |win, _| {
        win.destroy();
        Inhibit(false)
    });
    Ok(())
}

fn set_visual(window: &ApplicationWindow, _screen: &Option<gdk::Screen>) {
    if let Some(screen) = window.get_screen() {
        if let Some(visual) = screen.get_rgba_visual() {
            window.set_visual(&visual);
        }
    }
}

fn draw(_window: &ApplicationWindow, ctx: &cairo::Context) -> Inhibit {
    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.4);
    ctx.set_operator(cairo::enums::Operator::Screen);
    ctx.paint();
    Inhibit(false)
}

pub fn start_window() {
    let application =
        gtk::Application::new("com.drag_and_drop", gio::ApplicationFlags::empty())
            .expect("Initialization failed...");

    application.connect_startup(move |app| {
        match build_window(app) {
            Ok(_) => println!("Ok!"),
            Err(e) => println!("{:?}", e)
        };
    });
    application.connect_activate(|_| {});

    application.run(&args().collect::<Vec<_>>());
}
