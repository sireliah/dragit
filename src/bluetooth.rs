extern crate dbus;
use std::error::Error;
use std::collections::HashMap;
use self::dbus::{Connection, BusType, Message};
use self::dbus::arg::{Dict};

static OBEX_SERVICE_NAME: &'static str = "org.bluez.obex";
static OBEX_INTERFACE: &'static str = "org.bluez.obex.ObjectPush1";
static CLIENT_INTERFACE: &'static str = "org.bluez.obex.Client1";


pub fn create_session(object_path: &str) -> Result<bool, Box<Error>> {
    let mut map = HashMap::new();
    map.insert("Target", "opp");

    let c = try!(Connection::get_private(BusType::Session));
    let m = try!(Message::new_method_call(OBEX_SERVICE_NAME, "/org/bluez/obex", CLIENT_INTERFACE, "CreateSession"))
        .append2("B4:EB:F0:DB:9C:FB", Dict::new(&map));

    try!(c.send_with_reply_and_block(m, 1000));
    println!("Session established");
    Ok(true)
}


pub fn call_obex(object_path: &str) -> Result<bool, Box<Error>> {
    let c = try!(Connection::get_private(BusType::Session));
    let m = try!(Message::new_method_call(OBEX_SERVICE_NAME, object_path, OBEX_INTERFACE, "PutFile")).append1("./file.txt");;

    try!(c.send_with_reply_and_block(m, 1000));
    //let (data1, data2): (&str, i32) = try!(c.read());
    //println!("{}, {}", data1, data2);
    println!("Sent something");
    Ok(true)
}
