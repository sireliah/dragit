extern crate blurz;

use std::error::Error;
use std::process;
use std::thread::sleep;
use std::time::Duration;

use self::blurz::bluetooth_session::BluetoothSession as Session;
use self::blurz::bluetooth_obex::{
    BluetoothOBEXSession as OBEXSession, BluetoothOBEXTransfer as OBEXTransfer,
};
use self::blurz::BluetoothAdapter as Adapter;
use self::blurz::BluetoothDevice as Device;

use crate::transfer::Protocol;

pub struct BluetoothProtocol;

impl Protocol for BluetoothProtocol {
    fn transfer_file(&self, file_path: &str) -> Result<(), Box<dyn Error>> {
        info!("Received file to transfer: '{}'", file_path);
        let session = &Session::create_session(None)?;
        let adapter: Adapter = Adapter::init(session)?;
        let devices: Vec<String> = adapter.get_device_list()?;

        let filtered_devices = devices
            .iter()
            .filter(|&device_id| {
                let device = Device::new(session, device_id.to_string());
                device.is_ready_to_receive().expect("Could not check device")
            })
            .cloned()
            .collect::<Vec<String>>();

        let device_id: &str = &filtered_devices.get(0).expect("No devices found!");
        let device = Device::new(session, device_id.to_string());

        match connect(&device) {
            Ok(_) => match send_file_to_device(session, &device, file_path) {
                Ok(_) => info!("File sent to the device successfully."),
                Err(err) => error!("{:?}", err),
            },
            Err(err) => error!("{:?}", err),
        }
        Ok(())
    }
}

fn connect(device: &Device) -> Result<(), Box<dyn Error>> {
    let obex_push_uuid: String = "00001105-0000-1000-8000-00805f9b34fb"
        .to_string()
        .to_lowercase();
    info!(
        "Device name {:?}, Paired {:?}",
        device.get_name(),
        device.is_paired()
    );
    let uuids = device.get_uuids();
    info!("{:?}", uuids);
    device.connect(10000).ok();
    let push_func_found: bool = uuids?.contains(&obex_push_uuid);
    info!("Push file functionality found: {}", push_func_found);
    match device.is_connected() {
        Ok(_) => info!("Connected!"),
        Err(_) => process::exit(1),
    }
    sleep(Duration::from_millis(1000));
    Ok(())
}

fn send_file_to_device(session: &Session, device: &Device, file_path: &str) -> Result<(), Box<dyn Error>> {
    let session = OBEXSession::new(session, device)?;
    let transfer = OBEXTransfer::send_file(&session, file_path)?;

    match transfer.wait_until_transfer_completed() {
        Ok(_) => info!("Ok"),
        Err(error) => error!("{:?}", error),
    }

    session.remove_session()?;
    Ok(())
}
