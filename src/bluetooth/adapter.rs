extern crate blurz;

use std::error::Error;
use std::process;
use std::thread::sleep;
use std::time::Duration;

use self::blurz::bluetooth_obex::open_bus_connection;
use self::blurz::bluetooth_obex::{
    BluetoothOBEXSession as OBEXSession, BluetoothOBEXTransfer as OBEXTransfer,
};
use self::blurz::BluetoothAdapter as Adapter;
use self::blurz::BluetoothDevice as Device;

use crate::transfer::Protocol;

pub struct BluetoothProtocol;

impl Protocol for BluetoothProtocol {
    fn transfer_file(&self, file_path: &str) -> Result<(), Box<dyn Error>> {
        println!("Received file to transfer: '{}'", file_path);
        let adapter: Adapter = Adapter::init().unwrap();
        let devices: Vec<String> = adapter.get_device_list().unwrap();

        let filtered_devices = devices
            .iter()
            .filter(|&device_id| {
                let device = Device::new(device_id.to_string());
                device.is_ready_to_receive().unwrap()
            })
            .cloned()
            .collect::<Vec<String>>();

        let device_id: &str = &filtered_devices.get(0).expect("No devices found!");
        let device = Device::new(device_id.to_string());

        match connect(&device) {
            Ok(_) => match send_file_to_device(&device, file_path) {
                Ok(_) => println!("File sent to the device successfully."),
                Err(err) => println!("{:?}", err),
            },
            Err(err) => println!("{:?}", err),
        }

        Ok(())
    }
}

fn connect(device: &Device) -> Result<(), Box<dyn Error>> {
    let obex_push_uuid: String = "00001105-0000-1000-8000-00805f9b34fb"
        .to_string()
        .to_lowercase();
    println!(
        "Device name {:?}, Paired {:?}",
        device.get_name(),
        device.is_paired()
    );
    let uuids = device.get_uuids();
    println!("{:?}", uuids);
    device.connect().ok();
    let push_func_found: bool = uuids.unwrap().contains(&obex_push_uuid);
    println!("Push file functionality found: {}", push_func_found);
    match device.is_connected() {
        Ok(_) => println!("Connected!"),
        Err(_) => process::exit(1),
    }
    sleep(Duration::from_millis(1000));
    Ok(())
}

fn send_file_to_device(device: &Device, file_path: &str) -> Result<(), Box<dyn Error>> {
    let connection = open_bus_connection()?;
    let session = OBEXSession::new(connection, device)?;
    let transfer = OBEXTransfer::send_file(&session, file_path)?;

    match transfer.wait_until_transfer_completed() {
        Ok(_) => println!("Ok"),
        Err(error) => println!("{:?}", error),
    }
    Ok(())
}
