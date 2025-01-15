#[macro_use]
extern crate lazy_static;
mod data;
mod output;
mod tui;
mod device;

use std::io::{IsTerminal};
use std::sync::Arc;
use std::time::Duration;

use crate::data::{Command, Event};
use crate::output::{JSONPrinter, OutputFormat, OutputPrinter, TextPrinter};
use btleplug::api::Peripheral as _;
use btleplug::platform::Manager;
use clap::Parser;
use env_logger::Target::Pipe;
use env_logger::{Env, WriteStyle};
use log::{debug, info};
use rustyline_async::Readline;
use tokio::sync::broadcast;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
	/// Address of the device to use (e.g. 00:11:22:33:44:55)
	#[arg(short, long)]
	device: Option<String>,

	/// Output format (text, json)
	#[arg(short, long, default_value = "text")]
	format: OutputFormat,

	/// Skip checking for TTY and always run non-interactive.
	#[arg(long)]
	pipe: bool,

	/// Log level (error, warn, info, debug, trace)
	#[arg(long)]
	log_level: Option<log::LevelFilter>,

	/// Timeout to find the device, in seconds
	#[arg(long, default_value_t = 5.0)]
	scan_timeout: f32,

	/// Get battery level and SN on launch.
	#[arg(short, long)]
	get_status: bool,

	/// Calibrate on launch (instead of the initial scan)
	#[arg(short, long)]
	calibrate: bool,

	/// Scan on launch
	#[arg(short, long)]
	scan: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let args = Args::parse();

	let (btx, _brx) = broadcast::channel(64);

	let mut log_b = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
	
	let mut printer: Option<Box<dyn OutputPrinter>> = Some(match args.format {
		OutputFormat::Text => Box::new(TextPrinter {}),
		OutputFormat::JSON => Box::new(JSONPrinter {}),
	});

	let tui = if !args.pipe && std::io::stdin().is_terminal() {
		let prompt = match args.format {
			OutputFormat::Text => "> ",
			OutputFormat::JSON => "",
		}
		.to_owned();
		let (rl, stdout) = Readline::new(prompt)?;
		log_b
			.target(Pipe(Box::new(stdout.clone())))
			.write_style(WriteStyle::Always);
		Some(tui::tui_loop(rl, stdout.clone(), btx.clone(), printer.take().unwrap()))
	} else {
		log_b.write_style(WriteStyle::Never);
		None
	};

	if let Some(ll) = args.log_level {
		log_b.filter_level(ll);
	};
	log_b.init();

	let log_task = tokio::spawn(output::log_loop(btx.subscribe(), printer));

	let manager = Manager::new().await?;

	let tui = tui.map(tokio::spawn);

	let found = tokio::time::timeout(
		Duration::from_secs_f32(args.scan_timeout),
		device::find_device(manager, args.clone()),
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
		info!("Connecting");
		let res = device.connect().await;
		debug!("connect result: {:?}", res);
		res?;
	}
	info!("Connected");

	let dev_loop = tokio::spawn(device::device_loop(
		btx.subscribe(), // to ensure it exists before we start sending command line commands
		btx.clone(),
		device.clone(),
	));

	if args.get_status {
		debug!("Writing status commands");
		btx.send(Event::Command(Command::Status))?;
	}
	
	if args.calibrate {
		debug!("Writing calibrate command");
		btx.send(Event::Command(Command::Calibrate))?;
	}
	if args.scan {
		debug!("Writing scan command");
		btx.send(Event::Command(Command::Scan))?;
	}

	dev_loop.await??;
	log_task.await??;
	if let Some(task) = tui {
		task.await??.flush()?;
	}

	Ok(())
}

