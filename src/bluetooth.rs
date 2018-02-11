extern crate dbus;
use std::error::Error;
use std::collections::HashMap;
use self::dbus::{Connection, BusType, Message, MessageItem, Path, Props};
use self::dbus::arg::{Dict, Variant};

static OBEX_BUS: &'static str = "org.bluez.obex";
static OBEX_PATH: &'static str = "/org/bluez/obex";
static OBJECT_PUSH_INTERFACE: &'static str = "org.bluez.obex.ObjectPush1";
static CLIENT_INTERFACE: &'static str = "org.bluez.obex.Client1";
static TRANSFER_INTERFACE: &'static str = "org.bluez.obex.Transfer1";


pub fn open_bus_connection() -> Result<Connection, Box<Error>> {
    let c = try!(Connection::get_private(BusType::Session));

    Ok(c)
}


pub fn create_session<'z>(connection: &Connection, object_path: &str) -> Result<Path<'z>, Box<Error>> {

    println!("Trying to open session.");
    let device_address: &str = &object_path.replace("/org/bluez/hci0/dev_", "").replace("_", ":");
    let mut map = HashMap::new();
    map.insert("Target", Variant("opp"));
    let args: Dict<&str, Variant<&str>, _> = Dict::new(map);
    let m = try!(Message::new_method_call(OBEX_BUS, OBEX_PATH, CLIENT_INTERFACE, "CreateSession"))
        .append2(device_address, args);

    let r = try!(connection.send_with_reply_and_block(m, 1000));
    let session_path: Path = try!(r.read1());
    println!("Session opened: {}", session_path);
    Ok(session_path)
}


pub fn send_file<'z>(connection: &Connection, object_path: Path) -> Result<Path<'z>, Box<Error>> {
    let file_path: &str = "/home/sir/Aktywatory/dragit/dragit/src/file.txt";
    let m = try!(Message::new_method_call(OBEX_BUS, object_path, OBJECT_PUSH_INTERFACE, "SendFile"))
        .append1(file_path);
    println!("Trying to send file...");
    let r = try!(connection.send_with_reply_and_block(m, 1000));
    let transfer_path: Path = try!(r.read1());
    println!("Sent something {:?}", transfer_path);

    Ok(transfer_path)
}


pub fn check_transfer_status<'z>(connection: &Connection, object_path: Path) -> Result<bool, Box<Error>> {
    let p = Props::new(connection, OBEX_BUS, object_path, TRANSFER_INTERFACE, 1000);
    let status: MessageItem = try!(p.get("Status"));

    println!("Status {:?}", status);

    Ok(true)
}
