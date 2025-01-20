use crate::data::{Command, Event, ScanResult, Triple};
use crate::Args;
use btleplug::api::CentralEvent::DeviceDiscovered;
use btleplug::api::WriteType::WithoutResponse;
use btleplug::api::{BDAddr, Central, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter};
use btleplug::platform::{Manager, Peripheral};
use byteorder::ByteOrder;
use byteorder::{LittleEndian, ReadBytesExt};
use futures::FutureExt;
use log::{debug, error, info, trace, warn};
use std::collections::VecDeque;
use std::io::{BufRead, Cursor, Read};
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::select;
use tokio::sync::{broadcast, Mutex};
use tokio_stream::{StreamExt, StreamMap};
use uuid::Uuid;

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

	/// The command to trigger a calibration (result: AB202E00020000002DF4)
	static ref CALIBRATE_CMD: Vec<u8> = hex::decode("AB202E000200904F").unwrap();

	/// The command to request battery level
	static ref BATTERY_CMD: Vec<u8> = hex::decode("AB200B0002009B43").unwrap();

	/// The command to request device info
	static ref INFO_CMD: Vec<u8> = hex::decode("AB400000000014004504").unwrap();

}

pub async fn find_device(
	manager: Manager,
	args: &Args,
) -> Result<Option<(Peripheral, PeripheralProperties)>, anyhow::Error> {
	// Scan all BT adapters (not actually tested with more than one)
	let adapters = manager.adapters().await?;
	let mut scans = StreamMap::new();
	for (aidx, ad) in adapters.iter().enumerate() {
		scans.insert(aidx, ad.events().await?);
		ad.start_scan(ScanFilter::default()).await?;
	}

	let arg_addr = if let Some(str) = &args.device {
		Some(BDAddr::from_str(&str)?)
	} else {
		None
	};
	trace!("requested addr {arg_addr:?}");
	while let Some((aidx, ev)) = scans.next().await {
		trace!("event @{aidx} {ev:?}");
		if let DeviceDiscovered(pid) = ev {
			let ad = &adapters[aidx];
			let p = ad.peripheral(&pid).await?;
			if let Some(props) = p.properties().await? {
				let capable = props.services.contains(&WRITE_SVC_ID)
					&& props.services.contains(&NOTIF_SVC_ID);
				debug!(
					"device {} ({:?}), capable = {:?}",
					props.address, props.local_name, capable
				);
				// Only check for address if passed
				if let Some(addr) = arg_addr {
					if props.address == addr {
						return Ok(Some((p, props)));
					};
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

pub async fn device_loop(
	args: Args,
	mut brx: broadcast::Receiver<Event>,
	btx: broadcast::Sender<Event>,
) -> Result<Event, anyhow::Error> {
	debug!("starting device loop");
	
	let manager = Manager::new().await?;

	btx.send(Event::Connecting(None, None))?;
	
	let found = tokio::time::timeout(
		Duration::from_secs(args.find_timeout),
		find_device(manager, &args),
	)
	.await??;

	let (device, props) = found.ok_or(anyhow::Error::msg("No device found"))?;
	let device = Arc::new(device);
	if args.device.is_none() {
		info!(
			"Selected device: {} {:?}",
			device.address(),
			props.local_name
		);
	}

	let connected = device.is_connected().await?;
	info!("Connected = {connected}");
	if !connected {
		btx.send(Event::Connecting(Some(device.address().to_string()), props.local_name.clone()))?;
		info!("Connecting");
		let res = tokio::time::timeout(
			Duration::from_secs(args.connect_timeout),
			device.connect()
		).await;
		debug!("connect result: {:?}", res);
		res??;
	}
	
	btx.send(Event::Connected(device.address().to_string(), props.local_name.clone()))?;
	info!("Connected");

	device.discover_services().await?;
	let chars = device.characteristics();

	trace!("chars = {chars:?}");

	let notif_char = chars
		.iter()
		.find(|c| c.uuid == *NOTIF_CHR_ID)
		.ok_or(anyhow::Error::msg("No notif_char found"))?
		.clone();
	debug!("notif_char = {notif_char:?}");
	device.subscribe(&notif_char).await?;

	let write_char = chars
		.iter()
		.find(|c| c.uuid == *WRITE_CHR_ID)
		.ok_or(anyhow::Error::msg("No write_char found"))?
		.clone();
	let write_char_clone = write_char.clone();
	debug!("write_char = {write_char:?}");

	let waiting = Arc::new(AtomicBool::new(false));
	let commands = Arc::new(Mutex::new(VecDeque::<Vec<u8>>::new()));
	
	let try_cleanup = async || {
		if let Err(e) = device.unsubscribe(&notif_char).await {
			warn!("unsubscribe failed: {e:?}");
		}
		if let Err(e) = device.disconnect().await {
			warn!("disconnect failed: {e:?}");
		}		
	};

	let waiting_arc = waiting.clone();
	let device_arc = device.clone();
	let commands_arc = commands.clone();
	let enqueue_command = async move |cmd: &Vec<u8>| {
		let mut commands = commands_arc.lock().await;
		if commands.is_empty() && !waiting_arc.load(Relaxed) {
			debug!("write immediate command: {:x?}", cmd);
			let wres = device_arc.write(&write_char, cmd, WithoutResponse).await;
			if let Err(e) = wres {
				error!("write failed: {e:?}");
				try_cleanup().await;
				return Err(e.into());
			}
			waiting_arc.store(true, Relaxed);
		} else {
			commands.push_back(cmd.clone());
		}
		Ok::<(), anyhow::Error>(())
	};

	let commands_arc = commands.clone();
	let waiting_arc = waiting.clone();
	let device_arc = device.clone();

	let mut notif_stream = device.notifications().await?;
	
	let maybe_handle_command = async |cmd: Command| {
		match cmd {
			Command::Scan => {
				enqueue_command(&SCAN_CMD).await?;
			}
			Command::Calibrate => {
				enqueue_command(&CALIBRATE_CMD).await?;
			}
			Command::Status => {
				enqueue_command(&INFO_CMD).await?;
				enqueue_command(&BATTERY_CMD).await?;
			}
			_ => {}
		}
		Ok::<(), anyhow::Error>(())
	};

	let mut count: usize = 0;
	let mut last_result_at = Instant::now();
	let mut last_result_msg: Vec<u8> = Vec::new();
	loop {
		select! {
			btev = notif_stream.next().fuse() => match btev {
				Some(v) => {
					let msg = v.value;
					debug!("Received: {:x?}", msg);
					let [a, b, c] = msg[0..3] else {
						error!("Message too short: {:x?}", msg);
						continue;
					};
					if a != 0xAB {
						warn!("Unknown message: {:x?}", msg);
						continue;
					}

					if b == 0x44 {
						debug!("Is color scan result (AB44)");
						
						if msg == last_result_msg && Instant::now() - last_result_at < Duration::from_millis(300) {
							warn!("Duplicated result, dropping: {:x?}", msg);
						}
						else {
							count += 1;
							last_result_msg = msg.clone();
							last_result_at = Instant::now();
							let idx = count;
							let result = parse_scan_result(idx, msg);
							debug!("result = {result:?}");
							btx.send(Event::Scan(result))?;
						}
					} else if (b, c) == (0x20, 0x2E) {
						debug!("Is calibration response (AB202E)");
						btx.send(Event::Calibrated)?;
						// printer.format_misc("calibrated", true.into());
					} else if (b, c) == (0x20, 0x0B) {
						debug!("Is power level response (AB200B)");
						let level = LittleEndian::read_i16(&msg[6..8]);
						btx.send(Event::PowerLevel(level))?;
						// printer.format_misc("power_level", level.into());
					} else if (b, c) == (0x40, 0x00) {
						debug!("Is device info response (AB4000)");
						let device_info: Vec<i16> = (10..25)
							.map(|idx| LittleEndian::read_i16(&msg[idx..(idx + 2)]))
							.collect();
						btx.send(Event::DeviceInfo(device_info))?;
						// printer.format_misc("device_info", device_info.into());
					} else {
						warn!("Unknown message: {:x?}", msg);
					}

					let mut commands = commands_arc.lock().await;
					if let Some(cmd) = commands.pop_front() {
						debug!("write queued command: {:x?}", cmd);
						let wres = device_arc
							.write(&write_char_clone, &cmd, WithoutResponse)
							.await;
						if let Err(e) = wres {
							error!("write failed: {e:?}");
							try_cleanup().await;
							return Err(e.into());
						}
						waiting_arc.store(true, Relaxed);
					} else {
						waiting_arc.store(false, Relaxed);
					}
				},
				None => {
					btx.send(Event::Disconnected)?;
					try_cleanup().await;
					return Ok(Event::Disconnected);
				}
			},
			_ = tokio::time::sleep(Duration::from_secs(args.keepalive_interval)) => {
				enqueue_command(&BATTERY_CMD).await?;
			},
			ev = brx.recv() => match ev? {
				Event::Exit => {
					debug!("exiting dev_loop");
					try_cleanup().await;
					return Ok(Event::Exit);
				}
				Event::Command(cmd) => match cmd {
					Command::Disconnect => {
						debug!("disconnecting dev_loop");
						try_cleanup().await;
						btx.send(Event::Disconnected)?;
						return Ok(Event::Command(cmd));
					}
					_ => {
						maybe_handle_command(cmd).await?;
					}
				}
				Event::CommandQueue(q) => {
					for cmd in q {
						maybe_handle_command(cmd).await?;
					}
				}
				_ => {}
			}
		}
	}
}

fn parse_scan_result(idx: usize, msg: Vec<u8>) -> ScanResult {
	let mut cur = Cursor::new(msg);
	cur.consume(8);

	let mut read_floats = || {
		Triple(
			(0..3)
				.map(|_| (cur.read_i16::<LittleEndian>().unwrap() as f32) / 100.0)
				.collect::<Vec<f32>>()
				.try_into()
				.unwrap(),
		)
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

	ScanResult {
		idx,
		lab,
		luv,
		lch,
		yxy,
		rgb,
	}
}
