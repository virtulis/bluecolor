use crate::data::{Event, ScanResult, Triple};
use jzon::JsonValue;
use log::debug;
use std::str::FromStr;
use tokio::sync::broadcast;

#[derive(Clone, Copy, Debug)]
pub enum OutputFormat {
	Text,
	JSON,
}

impl FromStr for OutputFormat {
	type Err = String;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match &*s.to_ascii_lowercase() {
			"text" => Ok(Self::Text),
			"json" => Ok(Self::JSON),
			_ => Err(format!("Unknown output format: {s}"))
		}
	}
}

pub trait OutputPrinter: Send {
	fn format_event(&self, event: &Event) -> Option<String>;
}

pub struct TextPrinter;
impl OutputPrinter for TextPrinter {
	fn format_event(&self, event: &Event) -> Option<String> {
		match event {
			Event::Scan(res) => Some(vec![
				format!("Scan result #: {}", res.idx),
				format!("\tLab: {}", res.lab),
				format!("\tLuv: {}", res.luv),
				format!("\tLch: {}", res.lch),
				format!("\tyxY: {}", res.yxy),
				format!("\tRGB: {}", res.rgb),
			].join("\n")),
			Event::PowerLevel(val) => Some(format!("Power level: {val}")),
			Event::Error(str) => Some(format!("Error: {str}")),
			Event::Calibrated => Some("Calibrated".to_owned()),
			Event::Disconnected => Some("Disconnected".to_owned()),
			Event::Connected(addr, name) => Some(format!("Connected to {} ({})", addr, name.clone().unwrap_or("unnamed".to_owned()))),
			_ => None,
		}
	}
}

pub struct JSONPrinter;
impl JSONPrinter {
	pub fn format_result(&self, res: &ScanResult) -> JsonValue {
		let json_triple = |t: &Triple<f32>| JsonValue::Array(t.0.map(|n| JsonValue::Number(
			// These dances are the easiest way I found to strip the float noise
			jzon::number::Number::from_parts(n.is_sign_positive(), (n.abs() * 100.0).round() as u64, -2)
		)).into());
		let scan = jzon::object! {
			lab: json_triple(&res.lab),
			luv: json_triple(&res.luv),
			lch: json_triple(&res.lch),
			yxy: json_triple(&res.yxy),
			rgb: Vec::from(res.rgb.0),
		};
		jzon::object! { scan: scan }
	}
	pub fn format_event_json(&self, event: &Event) -> Option<JsonValue> {
		match event {
			Event::Exit => Some(jzon::array!["exit"]),
			Event::Error(str) => Some(jzon::array!["error", str.clone()]),
			Event::Scan(res) => Some(jzon::array!["scan", res.idx, self.format_result(&res)]),
			Event::Connecting(addr, name) => Some(jzon::array!["connecting", addr.clone(), name.clone()]),
			Event::Connected(addr, name) => Some(jzon::array!["connected", addr.clone(), name.clone()]),
			Event::Disconnected => Some(jzon::array!["disconnected"]),
			Event::PowerLevel(val) => Some(jzon::array!["power_level", val.clone()]),
			Event::DeviceInfo(val) => Some(jzon::array!["device_info", val.clone()]),
			Event::Calibrated => Some(jzon::array!["calibrated"]),
			Event::Command(_) => None,
			Event::CommandQueue(_) => None,
		}
	}
}
impl OutputPrinter for JSONPrinter {
	fn format_event(&self, event: &Event) -> Option<String> {
		self.format_event_json(event).map(|m| format!("{m}"))
	}
}

pub async fn log_loop(
	mut brx: broadcast::Receiver<Event>,
	printer: Option<Box<dyn OutputPrinter>>
) -> Result<(), anyhow::Error> {
	loop {
		match brx.recv().await? {
			Event::Exit => {
				break;
			}
			e => {
				debug!("event: {e:?}");
				let out = printer.as_ref().map(|p| p.format_event(&e)).flatten();
				if let Some(out) = out {
					println!("{}", out);
				}
			},
		}
	}
	Ok(())
}

