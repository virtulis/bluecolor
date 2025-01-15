use crate::data::{Command, Event};
use crate::output::OutputPrinter;
use futures::FutureExt;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use std::io::Write;
use tokio::select;
use tokio::sync::broadcast;

pub async fn tui_loop(
	mut rl: Readline,
	mut stdout: SharedWriter,
	btx: broadcast::Sender<Event>,
	printer: Box<dyn OutputPrinter>,
) -> Result<Readline, anyhow::Error> {
	
	let mut brx = btx.subscribe();

	loop {
		select! {
			line = rl.readline().fuse() => match line {
				Ok(ReadlineEvent::Line(str)) => {
					if let Some(e) = parse_tui_command(&str) {
						btx.send(e)?;
					}
				}
				Ok(ReadlineEvent::Eof) => {
					btx.send(Event::Exit)?;
				},
				Ok(ReadlineEvent::Interrupted) => {
					btx.send(Event::Exit)?;
				},
				Err(e) => {
					btx.send(Event::Error(format!("Readline: {e}")))?;
					break;
				}
			},
			Ok(event) = brx.recv() => match event {
				Event::Exit => {
					break;
				},
				ev => {
					let fmt = printer.format_event(&ev);
					if let Some(str) = fmt {
						stdout.write((str + "\n").as_bytes())?;
					}
				}
			}
		}
	}
	Ok(rl)
}

fn parse_tui_command(line: &str) -> Option<Event> {
	let mut split = line.trim().split_whitespace();
	match split.next() {
		None => None,
		Some(cmd) => match cmd.to_lowercase().as_str() {
			"exit" => Some(Event::Exit),
			"calibrate" => Some(Event::Command(Command::Calibrate)),
			"scan" => Some(Event::Command(Command::Scan)),
			"status" => Some(Event::Command(Command::Status)),
			_ => Some(Event::Error(format!("Unknown command: {}", cmd))),
		},
	}
}
