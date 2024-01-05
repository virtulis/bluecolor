use std::fmt::{Display, Formatter};
use std::str::FromStr;
use jzon::JsonValue;

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

#[derive(Debug)]
pub struct Triple<T: Display + Copy + Into<JsonValue>> (pub [T; 3]);
impl <T: Display + Copy + Into<JsonValue>> Display for Triple<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0.map(|n| n.to_string()).join(", "))
	}
}

#[derive(Debug)]
pub struct ScanResult {
	pub idx: usize,
	pub lab: Triple<f32>,
	pub luv: Triple<f32>,
	pub lch: Triple<f32>,
	pub yxy: Triple<f32>,
	pub rgb: Triple<u8>,
}
pub trait OutputPrinter: Send {
	fn print_result(&self, res: ScanResult);
}

pub struct TextPrinter;
impl OutputPrinter for TextPrinter {
	fn print_result(&self, res: ScanResult) {
		println!("Scan result #: {}", res.idx);
		println!("Lab: {}", res.lab);
		println!("Luv: {}", res.luv);
		println!("Lch: {}", res.lch);
		println!("yxY: {}", res.yxy);
		println!("RGB: {}", res.rgb);
	}
}

pub struct JSONPrinter;
impl OutputPrinter for JSONPrinter {
	fn print_result(&self, res: ScanResult) {
		let json_triple = |t: Triple<f32>| JsonValue::Array(t.0.map(|n| JsonValue::Number(
			// These dances are the easiest way I found to strip the float noise
			jzon::number::Number::from_parts(n.is_sign_positive(), (n.abs() * 100.0).round() as u64, -2)
		)).into()); 
		let obj = jzon::object! {
			lab: json_triple(res.lab),
			luv: json_triple(res.luv),
			lch: json_triple(res.lch),
			yxy: json_triple(res.yxy),
			rgb: Vec::from(res.rgb.0),
		};
		println!("{obj}");
	}
}

