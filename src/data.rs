use jzon::JsonValue;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq)]
pub struct Triple<T: Display + Copy + Into<JsonValue>> (pub [T; 3]);
impl <T: Display + Copy + Into<JsonValue>> Display for Triple<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0.map(|n| n.to_string()).join(", "))
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScanResult {
	pub idx: usize,
	pub lab: Triple<f32>,
	pub luv: Triple<f32>,
	pub lch: Triple<f32>,
	pub yxy: Triple<f32>,
	pub rgb: Triple<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
	Exit,
	Error(String),
	Scan(ScanResult),
	Connecting(Option<String>, Option<String>),
	Connected(String, Option<String>),
	Disconnected,
	PowerLevel(i16),
	DeviceInfo(Vec<i16>),
	Calibrated,
	Command(Command),
	CommandQueue(Vec<Command>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
	Scan,
	Calibrate,
	Status,
	Connect(String),
	Reconnect,
	Disconnect,
}

#[derive(Debug, Clone, Default)]
pub struct State {
	pub connected: bool,
	pub connecting: bool,
	pub device_address: Option<String>,
	pub device_name: Option<String>,
	pub power_level: Option<i16>,
	pub device_info_raw: Option<Vec<i16>>,
	pub calibrated: Option<std::time::SystemTime>,
}
