use std::fmt::{Display, Formatter};
use std::str::FromStr;
use jzon::JsonValue;
use crate::data::{Event, ScanResult, Triple};

#[derive(Clone, Copy, Debug)]
pub enum OutputFormat {
	Text,
	// TSV,
	JSON,
}

impl FromStr for OutputFormat {
	type Err = String;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match &*s.to_ascii_lowercase() {
			"text" => Ok(Self::Text),
			// "tsv" => Ok(Self::TSV),
			"json" => Ok(Self::JSON),
			_ => Err(format!("Unknown output format: {s}"))
		}
	}
}

pub trait OutputPrinter: Send {
	// fn format_result(&self, res: ScanResult) -> String;
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
}
impl OutputPrinter for JSONPrinter {
	fn format_event(&self, event: &Event) -> Option<String> {
		let msg = match event {
			Event::Exit => Some(jzon::array!["exit"]),
			Event::Error(str) => Some(jzon::array!["error", str.clone()]),
			Event::Scan(res) => Some(jzon::array!["scan", res.idx, self.format_result(&res)]),
			Event::Connecting => Some(jzon::array!["connecting"]),
			Event::Connected(addr, name) => Some(jzon::array!["connected", addr.clone(), name.clone()]),
			Event::Disconnected => Some(jzon::array!["disconnected"]),
			Event::PowerLevel(val) => Some(jzon::array!["power_level", val.clone()]),
			Event::DeviceInfo(val) => Some(jzon::array!["device_info", val.clone()]),
			Event::Calibrated => Some(jzon::array!["calibrated"]),
			Event::Command(_) => None,
		};
		msg.map(|m| format!("{m}"))
	}
	
}

