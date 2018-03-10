
extern crate gdk;
extern crate gio;
extern crate gtk;

use std::env::args;
use std::error::Error;
use std::thread;
use std::sync::Arc;

use std::path::Path;
use self::gtk::{ContainerExt, BoxExt, GtkWindowExt, Inhibit, WidgetExtManual, WidgetExt, LabelExt};
use self::gio::{ApplicationExt, ApplicationExtManual};

use bluetooth;


fn spawn_send_job(file_path: &str) {
    let trimmed_path = file_path.replace("file://", "").trim().to_string();
    let path_arc = Arc::new(trimmed_path);
    let path_clone = Arc::clone(&path_arc);

    thread::spawn(move || {
        println!("Spawning thread");
        match bluetooth::transfer_file(&path_clone) {
            Ok(value) => (),
            Err(err) => println!("{}", err)
        }
    });    
}

pub fn build_window(application: &gtk::Application) -> Result<(), Box<Error>> {
    let targets = vec![gtk::TargetEntry::new("STRING", gtk::TargetFlags::OTHER_APP, 0),
                       gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::OTHER_APP, 0)];
    let label = gtk::Label::new("Drop your file here");
    label.drag_dest_set(gtk::DestDefaults::ALL, &targets, gdk::DragAction::COPY);

    label.connect_drag_data_received(|w, _, _, _, s, _, _| {
        // println!("s: {:?},", &s.get_uris());

        let path: String = match s.get_text() {
            Some(value) => value,
            None => {
                s.get_uris().pop().unwrap()
            }
        };

        w.set_text(&path);

        if let Some(file_path) = Path::new(&path).to_str() {
            spawn_send_job(&file_path);
        } else {
            println!("Problem with the file path");   
        }
    });

    // Stack the button and label horizontally
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox.pack_start(&label, true, true, 0);

    // Finish populating the window and display everything
    let window = gtk::ApplicationWindow::new(application);
    window.set_title("Dragit!");
    window.set_default_size(600, 600);
    window.add(&hbox);
    window.show_all();

    let window_clone = window.clone();

    // GTK & main window boilerplate
    window.connect_delete_event(move |_, _| {
        window_clone.destroy();
        Inhibit(false)
    });
    Ok(())
}


pub fn start_window() -> Result<(), Box<Error>> {
    let application = gtk::Application::new("com.github.drag_and_drop",
                                            gio::ApplicationFlags::empty())
                                       .expect("Initialization failed...");

    application.connect_startup(move |app| {
        build_window(app);
    });
    application.connect_activate(|_| {});

    application.run(&args().collect::<Vec<_>>());
    Ok(())
}
