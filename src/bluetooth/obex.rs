extern crate dbus;
use self::dbus::arg::{Dict, Variant};
use self::dbus::{Connection, BusType, Message, MessageItem, Path, Props};
use std::collections::HashMap;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

pub use super::transfer_states;

static OBEX_BUS: &'static str = "org.bluez.obex";
static OBEX_PATH: &'static str = "/org/bluez/obex";
static OBJECT_PUSH_INTERFACE: &'static str = "org.bluez.obex.ObjectPush1";
static CLIENT_INTERFACE: &'static str = "org.bluez.obex.Client1";
static TRANSFER_INTERFACE: &'static str = "org.bluez.obex.Transfer1";


pub fn open_bus_connection() -> Result<Connection, Box<Error>> {
    let c = Connection::get_private(BusType::Session)?;
    Ok(c)
}

pub struct Session<'z> {
    connection: &'z Connection,
    object_path: Path<'z>
}

impl <'z> Session<'z> {
    pub fn new(connection: &'z Connection, object_path: &str) -> Result<Session<'z>, Box<Error>> {
        println!("Trying to open session.");
        let device_address: &str = &object_path.replace("/org/bluez/hci0/dev_", "").replace("_", ":");
        let mut map = HashMap::new();
        map.insert("Target", Variant("opp"));
        let args: Dict<&str, Variant<&str>, _> = Dict::new(map);
        let m = Message::new_method_call(OBEX_BUS, OBEX_PATH, CLIENT_INTERFACE, "CreateSession")?
            .append2(device_address, args);

        let r = connection.send_with_reply_and_block(m, 1000)?;
        let session_path: Path = r.read1()?;
        println!("Session opened: {}", session_path);

        let obex_session = Session { connection: connection, object_path: session_path };
        Ok(obex_session)
    }
}

pub struct Transfer<'z> {
    session: &'z Session<'z>,
    object_path: Path<'z>
}

impl <'z> Transfer<'z> {
    pub fn send_file(session: &'z Session, file_path: &str) -> Result<Transfer<'z>, Box<Error>> {
        /// TODO: check if the file exists.
        let session_path: Path = session.object_path.clone();
        let m = try!(Message::new_method_call(OBEX_BUS, session_path, OBJECT_PUSH_INTERFACE, "SendFile"))
            .append1(file_path);
        println!("Trying to send file...");
        let r = session.connection.send_with_reply_and_block(m, 1000)?;
        let transfer_path: Path = r.read1()?;
        println!("Sent something {:?}", transfer_path);

        let obex_transfer = Transfer { session: session, object_path: transfer_path };
        Ok(obex_transfer)
    }

    pub fn check_status(&self) -> Result<String, Box<Error>> {
        let transfer_path = self.object_path.clone();
        let p = Props::new(self.session.connection, OBEX_BUS, transfer_path, TRANSFER_INTERFACE, 1000);
        let status: MessageItem = p.get("Status")?;
        let transfer_status: String = status.inner::<&str>().unwrap().to_string();
        Ok(transfer_status)
    }

    pub fn wait_until_transfer_completed(&self) -> Result<(), Box<Error>>{
        sleep(Duration::from_millis(500));
        let mut transfer_status: String = self.check_status()?;

        while transfer_status != transfer_states::COMPLETE {
            println!("{}", transfer_status);
            sleep(Duration::from_millis(500));
            transfer_status = self.check_status()?;
        }
        Ok(())
    }

}
