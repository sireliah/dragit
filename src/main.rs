extern crate blurz;

use blurz::{BluetoothAdapter, BluetoothDevice};


fn connect(device: BluetoothDevice) {

    let obex_push_uuid: String = "00001105-0000-1000-8000-00805F9B34FB".to_string().to_lowercase();
    println!("Device name {:?}, Paired {:?}", device.get_name(), device.is_paired());
    let uuids = device.get_uuids();
    println!("{:?}", uuids);
    device.connect().ok();
    let push_func_found: bool = uuids.unwrap().contains(&obex_push_uuid);
    println!("Push file functionality found: {}", push_func_found);
    println!("Connected: {:?}", device.is_connected());

    std::thread::sleep(std::time::Duration::from_millis(1000));

    let services = device.get_gatt_services();
    let services_vec: Vec<String> = services.unwrap();
    println!("{:?}", services_vec);

    device.connect_profile(obex_push_uuid);

}

fn main() {
    let adapter: BluetoothAdapter = BluetoothAdapter::init().unwrap();
    let devices = adapter.get_device_list().unwrap(); 

    for device_id in devices {
        println!("Device: {}", device_id);

        let device = BluetoothDevice::new(device_id.clone());

        //let device_name = match device.get_name() {
        //    Ok(value) => value,
        //    Err(_) => println!("Failed connecting to device {}.", device_id),
        //};

        let device_name: String = device.get_name().unwrap();

        println!("{}", device_name);
        match device_name.as_ref() {
            "Kocham kota" => connect(device),
            _ => println!("Wrong device {}", device_name),
        };
    }
}

