#[macro_use]
extern crate lazy_static;
mod data;
mod device;
mod output;
mod server;
mod tui;

use std::io::IsTerminal;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use crate::data::{Command, Event};
use crate::output::{JSONPrinter, OutputFormat, OutputPrinter, TextPrinter};
use clap::Parser;
use env_logger::Target::Pipe;
use env_logger::{Env, WriteStyle};
use log::{debug, error};
use rustyline_async::Readline;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use crate::server::server_loop;

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
	#[arg(long, default_value_t = 10)]
	find_timeout: u64,
	
	/// Do not exit on disconnect or error
	#[arg(long)]
	remain: bool,
	
	/// Assume connect attempt failed if there is no result for that many seconds
	#[arg(long, default_value_t = 30)]
	connect_timeout: u64,
	
	/// Reconnect attempts (if all fail, give up until new commands are received)
	#[arg(long, default_value_t = 10)]
	reconnect_attempts: usize,
	
	/// Seconds to wait between reconnect attempts
	#[arg(long, default_value_t = 30)]
	reconnect_interval: u64,
	
	/// Send status command if connected but idle for that many seconds
	#[arg(long, default_value_t = 30)]
	keepalive_interval: u64,

	/// Get battery level and SN on launch.
	#[arg(short, long)]
	get_status: bool,

	/// Calibrate on launch (instead of the initial scan)
	#[arg(short, long)]
	calibrate: bool,

	/// Scan on launch
	#[arg(short, long)]
	scan: bool,

	/// Start a multi-tenant WebSocket server on this port
	#[arg(long, value_name = "PORT")]
	listen: Option<u16>,

	/// Websocket server host
	#[arg(long, default_value_t = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))]
	host: IpAddr,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let args = Args::parse();

	let (btx, mut brx) = broadcast::channel(64);

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
		Some(tui::tui_loop(
			rl,
			stdout.clone(),
			btx.clone(),
			printer.take().unwrap(),
		))
	} else {
		log_b.write_style(WriteStyle::Never);
		None
	};

	if let Some(ll) = args.log_level {
		log_b.filter_level(ll);
	};
	log_b.init();

	let log_task = tokio::spawn(output::log_loop(btx.subscribe(), printer));

	let tui = tui.map(tokio::spawn);
	
	let server = args.listen.map(|port| {
		tokio::spawn(server_loop(btx.clone(), SocketAddr::from((args.host, port))))
	});
	
	let mut command_queue: Vec<Command> = Vec::new();
	if args.get_status {
		command_queue.push(Command::Status);
	}
	if args.calibrate {
		command_queue.push(Command::Calibrate);
	}
	if args.scan {
		command_queue.push(Command::Scan);
	}
	
	let mut dev_loop: Option<JoinHandle<_>> = None;
	let mut try_connecting = true;
	let mut attempts = 0;
	
	loop {
		
		debug!("dev_loop ? {} ; try_connecting = {try_connecting} ; attempts = {attempts}", dev_loop.is_some());
		
		if dev_loop.is_none() && try_connecting {
			attempts += 1;
			debug!("Connection attempt {attempts}");
			dev_loop = Some(tokio::spawn(device::device_loop(
				args.clone(),
				btx.subscribe(), // to ensure it exists before we start sending command line commands
				btx.clone(),
			))).into();
			if !command_queue.is_empty() {
				btx.send(Event::CommandQueue(command_queue.clone()))?;
			}
		}
		
		if let Some(task) = dev_loop.take() {
			match task.await? {
				Ok(ev) => match ev {
					Event::Exit => {
						debug!("exiting 1");
						break;
					}
					Event::Disconnected => {
						attempts = 0; // had a successful connection, reset counter
						command_queue.clear();
					}
					_ => {
						debug!("device_loop exited with: {:?}", ev);
					}
				},
				Err(e) => {
					debug!("device_loop exited with: {:?}", e);
					error!("device_loop error: {e}");
					command_queue.clear();
				}
			}
			try_connecting = args.remain && attempts < args.reconnect_attempts;
			if try_connecting && attempts > 1 {
				tokio::time::sleep(Duration::from_secs(args.reconnect_interval)).await;
			}
		}
		else {
			match brx.recv().await? {
				Event::Command(cmd) => match cmd {
					Command::Reconnect => {
						attempts = 0;
						try_connecting = true;
					}
					Command::Scan | Command::Calibrate | Command::Status => {
						attempts = 0;
						try_connecting = true;
						command_queue.push(cmd);
					}
					_ => {
						btx.send(Event::Error("Device is disconnected".to_owned()))?;
					}
				}
				Event::Exit => {
					debug!("exiting 2");
					break;
				}
				_ => {}
			}
		}
		
	}
	
	debug!("await log_task");
	log_task.await??;
	
	if let Some(task) = server {
		debug!("await server");
		task.await??;
	}
	
	if let Some(task) = tui {
		debug!("await tui");
		task.await??.flush()?;
	}

	Ok(())
}
