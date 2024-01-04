#[macro_use]
extern crate lazy_static;

use std::error::Error;
use std::io::{BufRead, Cursor};
use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::api::WriteType::WithoutResponse;
use btleplug::platform::{Adapter, Manager, Peripheral};
use byteorder::{LittleEndian, ReadBytesExt};
use tokio::time;
use tokio_stream::StreamExt;
use uuid::Uuid;

lazy_static! {
	static ref WRITE_SVC_ID: Uuid = Uuid::parse_str("0000ffe5-0000-1000-8000-00805f9b34fb").unwrap();
	static ref WRITE_CHR_ID: Uuid = Uuid::parse_str("0000ffe9-0000-1000-8000-00805f9b34fb").unwrap();
	static ref NOTIF_SVC_ID: Uuid = Uuid::parse_str("0000ffe0-0000-1000-8000-00805f9b34fb").unwrap();
	static ref NOTIF_CHR_ID: Uuid = Uuid::parse_str("0000ffe4-0000-1000-8000-00805f9b34fb").unwrap();
	static ref PAIR_CMD: Vec<u8> = hex::decode("AB440000000036001864").unwrap();
	static ref SCAN_CMD: Vec<u8> = hex::decode("AB200B0002009B43").unwrap();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	
	let manager = Manager::new().await.unwrap();
	
	// get the first bluetooth adapter
	let adapters = manager.adapters().await?;
	let central = adapters.into_iter().nth(0).unwrap();
	
	// start scanning for devices
	central.start_scan(ScanFilter::default()).await?;
	// instead of waiting, you can use central.events() to get a stream which will
	// notify you of new devices, for an example of that see examples/event_driven_discovery.rs
	time::sleep(Duration::from_secs(2)).await;
	
	// find the device we're interested in
	let found = find_light(&central).await;
	if found.is_none() {
		println!("No device found");
		return Ok(());
	}
	let device = found.unwrap();
	
	// connect to the device
	device.connect().await?;
	
	// discover services and characteristics
	device.discover_services().await?;
	
	// find the characteristic we want
	let chars = device.characteristics();
	
	// println!("{chars:?}");
	
	let notif_char = chars.iter().find(|c| c.uuid == *NOTIF_CHR_ID).unwrap();
	device.subscribe(notif_char).await?;
	
	let mut notif_stream = device.notifications().await?;
	let notif = tokio::spawn(async move {
		while let Some(v) = notif_stream.next().await {
			println!("GOT = {:?}", v);
			if v.value[0] == 0xAB && v.value[1] == 0x44 {
				let mut cur = Cursor::new(v.value);
				cur.consume(8);
				let mut read_floats = || { (0..3).map(|_| {
					(cur.read_i16::<LittleEndian>().unwrap() as f32) / 100.0
				}).collect::<Vec<f32>>() };
				let lab = read_floats();
				let luv = read_floats();
				let lch = read_floats();
				let yxy = read_floats();
				let mut read_bytes = |len: usize| { (0..len).map(|_| { cur.read_u8().unwrap() }).collect::<Vec<u8>>() };
				let cmyk = read_bytes(4);
				let rgb = read_bytes(3);
				println!("Lab: {lab:?}, Luv: {luv:?}, Lch: {lch:?}, yxY: {yxy:?}, CMYK: {cmyk:?}, RGB: {rgb:?}");
			}
		}
	});
	
	let write_char = chars.iter().find(|c| c.uuid == *WRITE_CHR_ID).unwrap();
	device.write(write_char, &PAIR_CMD, WithoutResponse).await?;
	device.write(write_char, &SCAN_CMD, WithoutResponse).await?;
	
	notif.await.unwrap();
	
	Ok(())
	
}

async fn find_light(central: &Adapter) -> Option<Peripheral> {
	for p in central.peripherals().await.unwrap() {
		let props = p.properties().await.unwrap().unwrap();
		println!("{props:?}");
		if props.services.contains(&WRITE_SVC_ID) && props.services.contains(&NOTIF_SVC_ID) {
			return Some(p);
		}
	}
	None
}
