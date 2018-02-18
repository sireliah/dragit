extern crate blurz;

use std::process;
use std::error::Error;
use blurz::{BluetoothAdapter, BluetoothDevice};
mod bluetooth;
pub mod transfer_states;


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
    std::thread::sleep(std::time::Duration::from_millis(1000));
    Ok(())
}


fn send_file_to_device(device: &BluetoothDevice) -> Result<(), Box<Error>> {
    let device_id: String = device.get_id();
    let connection = bluetooth::open_bus_connection()?;
    let session_path = bluetooth::create_session(&connection, &device_id)?;

    let file_path: &str = "/home/sir/Aktywatory/dragit/dragit/src/file.txt";
    let transfer_path = bluetooth::send_file(&connection, session_path, file_path)?;

    std::thread::sleep(std::time::Duration::from_millis(5000));

    match bluetooth::wait_until_transfer_completed(&connection, &transfer_path) {
        Ok(_) => println!("Ok"),
        Err(error) => println!("{:?}", error)
    }
    Ok(())
}


fn main() {
    let adapter: BluetoothAdapter = BluetoothAdapter::init().unwrap();
    let devices = adapter.get_device_list().unwrap(); 

    for device_id in devices {
        println!("Device_id: {}", device_id);

        let device = BluetoothDevice::new(device_id.clone());

        match device_id.as_ref() {
            "/org/bluez/hci0/dev_00_00_00_00_5A_AD" => match connect(&device) {
                Ok(_) => {
                    match send_file_to_device(&device) {
                        Ok(_) => println!("File sent to the device successfully."),
                        Err(err) => println!("{:?}", err)
                    }
                }
                Err(err) => println!("{:?}", err)
            },
            _ => println!("Wrong device {}", device_id)
        }

    }
}

