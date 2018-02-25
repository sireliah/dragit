extern crate blurz;

use std::error::Error;
use std::process;
use std::thread::sleep;
use std::time::Duration;

use self::blurz::{BluetoothAdapter, BluetoothDevice};
use super::obex;


pub fn transfer_file(file_path: &str) -> Result<(), Box<Error>> {
    println!("Received file to transfer: '{}'", file_path);
    let adapter: BluetoothAdapter = BluetoothAdapter::init().unwrap();
    let devices: Vec<String> = adapter.get_device_list().unwrap();

    let filtered_devices = devices.iter()
                                  .filter(|&device_id| is_ready_to_receive(device_id).unwrap())
                                  .cloned()
                                  .collect::<Vec<String>>();

    let device_id: &str = match filtered_devices.iter().nth(0) {
        Some(value) => value,
        None => return Err(From::from("No device found!".to_string()))
    };
    let device = BluetoothDevice::new(device_id.to_string());

    match connect(&device) {
        Ok(_) => {
            match send_file_to_device(&device, file_path) {
                Ok(_) => println!("File sent to the device successfully."),
                Err(err) => println!("{:?}", err)
            }
        }
        Err(err) => println!("{:?}", err)
    }

    Ok(())
}


fn connect(device: &BluetoothDevice) -> Result<(), Box<Error>> {
    let obex_push_uuid: String = "00001105-0000-1000-8000-00805f9b34fb".to_string().to_lowercase();
    println!("Device name {:?}, Paired {:?}", device.get_name(), device.is_paired());
    let uuids = device.get_uuids();
    println!("{:?}", uuids);
    device.connect().ok();
    let push_func_found: bool = uuids.unwrap().contains(&obex_push_uuid);
    println!("Push file functionality found: {}", push_func_found);
    match device.is_connected() {
        Ok(_) => println!("Connected!"),
        Err(_) => process::exit(1)
    }
    sleep(Duration::from_millis(1000));
    Ok(())
}


fn send_file_to_device(device: &BluetoothDevice, file_path: &str) -> Result<(), Box<Error>> {
    let device_id: String = device.get_id();
    let connection = obex::open_bus_connection()?;
    let session_path = obex::create_session(&connection, &device_id)?;
    let transfer_path = obex::send_file(&connection, session_path, file_path)?;

    sleep(Duration::from_millis(5000));

    match obex::wait_until_transfer_completed(&connection, &transfer_path) {
        Ok(_) => println!("Ok"),
        Err(error) => println!("{:?}", error)
    }
    Ok(())
}


fn is_ready_to_receive(device_id: &str) -> Option<bool> {
    /// Check if the device is paired and currently connected
    let device = BluetoothDevice::new(device_id.to_string());
    let is_connected: bool = match device.is_connected() {
        Ok(value) => value,
        Err(err) => {
            println!("{:?}", err);
            false
        }
    };
    let is_paired: bool = match device.is_paired() {
        Ok(value) => value,
        Err(err) => {
            println!("{:?}", err);
            false
        }
    };
    Some(is_paired & is_connected)
}
