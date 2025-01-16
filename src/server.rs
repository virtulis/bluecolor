use std::net::SocketAddr;
use futures::future::select;
use futures::{FutureExt, StreamExt};
use log::{debug, info};
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::protocol::Message;
use crate::data::Event;

pub async fn server_loop(
	btx: broadcast::Sender<Event>,
	addr: SocketAddr
) -> Result<(), anyhow::Error> {
	let mut brx = btx.subscribe();
	let listener = TcpListener::bind(&addr).await?;
	info!("Listening on: {}", addr);
	loop {
		select! {
			Ok((stream, addr)) = listener.accept().fuse() => {
				tokio::spawn(connection_loop(stream, addr));
			},
			Ok(e) = brx.recv() => match e {
				Event::Exit => {
					break;
				}
				_ => {}
			}
		}
	}
	Ok(())
}

pub async fn connection_loop(
	raw_stream: TcpStream,
	addr: SocketAddr
) -> Result<(), anyhow::Error> {
	debug!("Accepting connection from: {}", addr);
	let ws_stream = tokio_tungstenite::accept_async(raw_stream).await?;
	let (send, mut recv) = ws_stream.split();
	info!("Connection from: {}", addr);
	loop {
		select! {
			msg = recv.next() => match msg {
				None => { break },
				Some(Err(e)) => {
					debug!("Connection {addr} error {:?}", e);
					break;
				},
				Some(Ok(Message::Text(text))) => {
					debug!("From {addr}: {text}");
					let msg = jzon::parse(text.as_str());
					debug!("Parsed from {addr}: {msg:?}");
				},
				Some(Ok(msg)) => {
					debug!("Unsupported message type from {}: {:?}", addr, msg);
				}
			}
		}
	}
	info!("Connection closed: {}", addr);
	Ok(())
}
