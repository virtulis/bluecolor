use crate::data::{Command, Event, State};
use futures::{FutureExt, SinkExt, StreamExt};
use jzon::JsonValue;
use log::{debug, info, warn};
use std::net::SocketAddr;
use chrono::Utc;
use futures::stream::SplitSink;
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::protocol::Message;
use crate::output::{JSONPrinter, OutputPrinter};

pub async fn server_loop(
	btx: broadcast::Sender<Event>,
	addr: SocketAddr,
) -> Result<(), anyhow::Error> {
	let mut state = State::default();
	let mut brx = btx.subscribe();
	let listener = TcpListener::bind(&addr).await?;
	info!("Listening on: {}", addr);
	loop {
		select! {
			Ok((stream, addr)) = listener.accept().fuse() => {
				tokio::spawn(connection_loop(stream, addr, btx.clone(), state.clone()).then(async |res| {
					if let Err(e) = &res {
						warn!("connection error: {}", e);
					}
				}));
			},
			Ok(e) = brx.recv() => match e {
				Event::Exit => {
					break;
				}
				Event::Connected(addr, name) => {
					state.connecting = false;
					state.connected = true;
					state.device_address = Some(addr);
					state.device_name = name;
				}
				Event::Connecting(addr, name) => {
					state.connecting = true;
					state.connected = false;
					state.device_address = addr;
					state.device_name = name;
				}
				Event::Disconnected => {
					state.connected = false;
				}
				Event::PowerLevel(val) => {
					state.power_level = Some(val);
				}
				Event::Calibrated => {
					state.calibrated = Some(std::time::SystemTime::now());
				}
				Event::DeviceInfo(raw) => {
					state.device_info_raw = Some(raw);
				}
				_ => {}
			}
		}
	}
	Ok(())
}

pub async fn connection_loop(
	raw_stream: TcpStream,
	addr: SocketAddr,
	btx: broadcast::Sender<Event>,
	init_state: State,
) -> Result<(), anyhow::Error> {
	debug!("Accepting connection from: {}", addr);
	let ws_stream = tokio_tungstenite::accept_async(raw_stream).await?;
	let (mut tx, mut rx) = ws_stream.split();
	info!("Connection from: {}", addr);
	let mut brx = btx.subscribe();
	let mut send_json = async |send: &mut SplitSink<_, _>, msg: JsonValue| {
		send.send(Message::Text(msg.to_string().into())).await
	};
	let mut wrong = async |send: &mut _| {
		send_json(send, jzon::array!["error", "invalid message"]).await
	};
	send_json(&mut tx, jzon::array!["state", jzon::object!{
		connected: init_state.connected,
		connecting: init_state.connecting,
		device_address: init_state.device_address.clone(),
		device_name: init_state.device_name.clone(),
		power_level: init_state.power_level,
		calibrated: init_state.calibrated.map(|t| { chrono::DateTime::<Utc>::from(t).to_rfc3339() }),
	}]).await?;
	drop(init_state);
	let printer = JSONPrinter {};
	loop {
		select! {
			msg = rx.next() => match msg {
				None => { break },
				Some(Err(e)) => {
					debug!("Connection {addr} error {:?}", e);
					break;
				},
				Some(Ok(Message::Text(text))) => {
					debug!("From {addr}: {text}");
					let msg = jzon::parse(text.as_str());
					debug!("Parsed from {addr}: {msg:?}");
					if let JsonValue::Array(arr) = msg? {
						let Some((cmd, args)) = arr.split_first() else {
							wrong(&mut tx).await?;
							continue;
						};
						let Some(str) = cmd.as_str() else {
							wrong(&mut tx).await?;
							continue;
						};
						let ev = match str {
							"exit" => Some(Event::Exit),
							"calibrate" => Some(Event::Command(Command::Calibrate)),
							"scan" => Some(Event::Command(Command::Scan)),
							"status" => Some(Event::Command(Command::Status)),
							"disconnect" => Some(Event::Command(Command::Disconnect)),
							"reconnect" => Some(Event::Command(Command::Reconnect)),
							_ => {
								send_json(&mut tx, jzon::array!["error", "invalid command"]).await?;
								None
							},
						};
						if let Some(ev) = ev {
							btx.send(ev)?;
						}
					}
				},
				Some(Ok(msg)) => {
					wrong(&mut tx).await?;
				}
			},
			Ok(e) = brx.recv() => match e {
				Event::Exit => {
					break;
				},
				ev => {
					let fmt = printer.format_event_json(&ev);
					if let Some(val) = fmt {
						send_json(&mut tx, val).await?;
					}
				}
			}
		}
	}
	info!("Connection closed: {}", addr);
	Ok(())
}
