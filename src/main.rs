mod output;

#[macro_use]
extern crate lazy_static;

use std::io::{BufRead, Cursor, Read};
use std::str::FromStr;
use std::time::Duration;

use btleplug::api::{BDAddr, Central, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter};
use btleplug::api::CentralEvent::DeviceDiscovered;
use btleplug::api::WriteType::WithoutResponse;
use btleplug::platform::{Manager, Peripheral};
use byteorder::{LittleEndian, ReadBytesExt};
use clap::Parser;
use env_logger::Env;
use log::{debug, info, trace};
use tokio_stream::{StreamExt, StreamMap};
use uuid::Uuid;
use crate::output::{JSONPrinter, OutputFormat, OutputPrinter, ScanResult, TextPrinter, Triple};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
	/// Address of the device to use (e.g. 00:11:22:33:44:55)
	#[arg(short, long)]
	device: Option<String>,
	
	/// Output format (text, json)
	#[arg(short, long, default_value = "text")]
	format: OutputFormat,
	
	/// Log level (error, warn, info, debug, trace)
	#[arg(long)]
	log_level: Option<log::LevelFilter>,
	
	/// Timeout to find the device, in seconds
	#[arg(long, default_value_t = 5.0)]
	scan_timeout: f32,
	
}

lazy_static! {
	
	/// Command destination service
	static ref WRITE_SVC_ID: Uuid = Uuid::parse_str("0000ffe5-0000-1000-8000-00805f9b34fb").unwrap();
	/// Command destination characteristic
	static ref WRITE_CHR_ID: Uuid = Uuid::parse_str("0000ffe9-0000-1000-8000-00805f9b34fb").unwrap();
	
	/// Notification service
	static ref NOTIF_SVC_ID: Uuid = Uuid::parse_str("0000ffe0-0000-1000-8000-00805f9b34fb").unwrap();
	/// Notification characteristic
	static ref NOTIF_CHR_ID: Uuid = Uuid::parse_str("0000ffe4-0000-1000-8000-00805f9b34fb").unwrap();
	
	/// The command to trigger a color scan (results sent as AB44... notification)
	static ref SCAN_CMD: Vec<u8> = hex::decode("AB440000000036001864").unwrap();
	
	
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let args = Args::parse();
	
	let mut log_b = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
	if let Some(ll) = args.log_level { log_b.filter_level(ll); };
	log_b.init();
	
	let printer: Box<dyn OutputPrinter> = match args.format {
		OutputFormat::Text => Box::new(TextPrinter {}),
		OutputFormat::JSON => Box::new(JSONPrinter {}),
	};
	
	let manager = Manager::new().await?;
	
	let found = tokio::time::timeout(
		Duration::from_secs_f32(args.scan_timeout),
		find_device(manager, args.clone()),
	).await??;
	let (device, props) = found.ok_or(anyhow::Error::msg("No device found"))?;
	if args.device.is_none() {
		info!("Selected device: {} {:?}", device.address(), props.local_name);
	}
	
	let connected = device.is_connected().await?;
	debug!("Connected = {connected}");
	if !connected {
		debug!("Connecting");
		device.connect().await?;
	}
	debug!("Connected");
	
	device.discover_services().await?;
	let chars = device.characteristics();
	
	trace!("chars = {chars:?}");
	
	let notif_char = chars.iter().find(|c| c.uuid == *NOTIF_CHR_ID).unwrap();
	trace!("notif_char = {notif_char:?}");
	device.subscribe(notif_char).await?;
	
	let mut notif_stream = device.notifications().await?;
	let notif = tokio::spawn(async move {
		let mut count: usize = 0;
		while let Some(v) = notif_stream.next().await {
			debug!("Received: {:?}", v);
			if v.value[0] == 0xAB && v.value[1] == 0x44 {
				debug!("Is color scan result");
				
				count += 1;
				let idx = count;
				
				let mut cur = Cursor::new(v.value);
				cur.consume(8);
				
				let mut read_floats = || {
					Triple((0..3).map(|_| {
						(cur.read_i16::<LittleEndian>().unwrap() as f32) / 100.0
					}).collect::<Vec<f32>>().try_into().unwrap())
				};
				let lab = read_floats();
				let luv = read_floats();
				let lch = read_floats();
				let yxy = read_floats();
				
				// Some arbitrary CMYK here. Useless in practice.
				cur.consume(4);
				
				let mut rgb_arr: [u8; 3] = [0; 3];
				cur.read_exact(&mut rgb_arr).unwrap();
				let rgb = Triple(rgb_arr);
				
				let result = ScanResult { idx, lab, luv, lch, yxy, rgb };
				
				debug!("result = {result:?}");
				printer.print_result(result);
				
			}
		}
	});
	
	let write_char = chars.iter().find(|c| c.uuid == *WRITE_CHR_ID).unwrap();
	trace!("write_char = {write_char:?}");
	
	debug!("Writing scan command");
	device.write(write_char, &SCAN_CMD, WithoutResponse).await?;
	
	notif.await.unwrap();
	
	Ok(())
}

async fn find_device(manager: Manager, args: Args) -> Result<Option<(Peripheral, PeripheralProperties)>, anyhow::Error> {
	
	// Scan all BT adapters (not actually tested with more than one)
	let adapters = manager.adapters().await?;
	let mut scans = StreamMap::new();
	for (aidx, ad) in adapters.iter().enumerate() {
		scans.insert(aidx, ad.events().await?);
		ad.start_scan(ScanFilter::default()).await?;
	}
	
	let arg_addr = if let Some(str) = args.device { Some(BDAddr::from_str(&str)?) } else { None };
	trace!("requested addr {arg_addr:?}");
	while let Some((aidx, ev)) = scans.next().await {
		trace!("event @{aidx} {ev:?}");
		if let DeviceDiscovered(pid) = ev {
			let ad = &adapters[aidx];
			let p = ad.peripheral(&pid).await?;
			if let Some(props) = p.properties().await? {
				let capable = props.services.contains(&WRITE_SVC_ID) && props.services.contains(&NOTIF_SVC_ID);
				debug!("device {} ({:?}), capable = {:?}", props.address, props.local_name, capable);
				// Only check for address if passed
				if let Some(addr) = arg_addr {
					if props.address == addr { return Ok(Some((p, props))); };
				}
				// Otherwise return first capable
				else if capable {
					return Ok(Some((p, props)));
				}
			}
		}
	}
	
	Ok(None)
}
