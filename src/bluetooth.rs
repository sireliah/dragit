extern crate dbus;
use std::error::Error;
use std::collections::HashMap;
use self::dbus::{Connection, BusType, Message, Path};
use self::dbus::arg::{Array, Dict, Variant};

static OBEX_BUS: &'static str = "org.bluez.obex";
static OBEX_PATH: &'static str = "/org/bluez/obex";
static OBEX_INTERFACE: &'static str = "org.bluez.obex.ObjectPush1";
static CLIENT_INTERFACE: &'static str = "org.bluez.obex.Client1";


pub fn create_session(object_path: &str) -> Result<Path, Box<Error>> {
    let mut map = HashMap::new();

    map.insert("Target", Variant("opp"));

    let args: Dict<&str, Variant<&str>, _> = Dict::new(map);

    let c = try!(Connection::get_private(BusType::Session));
    let m = try!(Message::new_method_call(OBEX_BUS, OBEX_PATH, CLIENT_INTERFACE, "CreateSession"))
        .append2("00:00:00:00:5A:AD", args);

    let r = try!(c.send_with_reply_and_block(m, 1000));
    let p: Path = r.get1().unwrap();
    println!("Session established! {}", p);
    Ok(p)
}


pub fn call_obex(object_path: &str) -> Result<bool, Box<Error>> {
    let c = try!(Connection::get_private(BusType::Session));
    let m = try!(Message::new_method_call(OBEX_BUS, object_path, OBEX_INTERFACE, "PutFile")).append1("./file.txt");;

    try!(c.send_with_reply_and_block(m, 1000));
    //let (data1, data2): (&str, i32) = try!(c.read());
    //println!("{}, {}", data1, data2);
    println!("Sent something");
    Ok(true)
}
