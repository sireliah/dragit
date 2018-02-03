extern crate blurz;

use std::process;
use blurz::{BluetoothAdapter, BluetoothDevice};
mod bluetooth;


fn connect(device: BluetoothDevice) {

    let obex_push_uuid: String = "00001105-0000-1000-8000-00805F9B34FB".to_string().to_lowercase();
    println!("Device name {:?}, Paired {:?}", device.get_name(), device.is_paired());
    let uuids = device.get_uuids();
    println!("{:?}", uuids);
    device.connect().ok();
    let push_func_found: bool = uuids.unwrap().contains(&obex_push_uuid);
    println!("Push file functionality found: {}", push_func_found);
    match device.is_connected() {
        Ok(_) => println!("Connected!"),
        Err(err) => process::exit(1)
    }
    std::thread::sleep(std::time::Duration::from_millis(1000));

    let services = device.get_gatt_services();
    let services_vec: Vec<String> = services.unwrap();
    println!("{:?}", services_vec);

    let device_id: String = device.get_id();

    let session = bluetooth::create_session(&device_id);
    println!("{}", session.unwrap());

    // let result = bluetooth::call_obex(&device.get_id());
    // println!("{}", result.unwrap());

}

fn main() {
    let adapter: BluetoothAdapter = BluetoothAdapter::init().unwrap();
    let devices = adapter.get_device_list().unwrap(); 

    for device_id in devices {
        println!("Device_id: {}", device_id);

        let device = BluetoothDevice::new(device_id.clone());

        match device_id.as_ref() {
            "/org/bluez/hci0/dev_00_00_00_00_5A_AD" => connect(device),
            _ => println!("Wrong device {}", device_id)
        }

    }
}

