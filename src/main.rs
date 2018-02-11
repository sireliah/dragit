extern crate blurz;

use std::process;
use std::error::Error;
use blurz::{BluetoothAdapter, BluetoothDevice};
mod bluetooth;


// fn wait_until_transfer_completed(transfer_status: &str) {

//     while transfer_status != "complete" {
//         std::thread::sleep(std::time::Duration::from_millis(500));
//         transfer_status: &str = try!(bluetooth::check_transfer_status(&connection, transfer_path));
//     }
// }


fn connect(device: BluetoothDevice) -> Result<(), Box<Error>>{
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

    // let services = device.get_gatt_services();
    // let services_vec: Vec<String> = services.unwrap();
    // println!("{:?}", services_vec);

    let device_id: String = device.get_id();

    // TODO: handle error here

    let connection = try!(bluetooth::open_bus_connection());
    let session_path = try!(bluetooth::create_session(&connection, &device_id));
    let transfer_path = try!(bluetooth::send_file(&connection, session_path));

    try!(bluetooth::check_transfer_status(&connection, transfer_path));

    std::thread::sleep(std::time::Duration::from_millis(5000));

    // wait_until_transfer_completed(transfer_status);

    Ok(())
}

fn main() {
    let adapter: BluetoothAdapter = BluetoothAdapter::init().unwrap();
    let devices = adapter.get_device_list().unwrap(); 

    for device_id in devices {
        println!("Device_id: {}", device_id);

        let device = BluetoothDevice::new(device_id.clone());

        match device_id.as_ref() {
            "/org/bluez/hci0/dev_00_00_00_00_5A_AD" => connect(device).unwrap(),
            _ => println!("Wrong device {}", device_id)
        }

    }
}

