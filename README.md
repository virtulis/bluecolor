# bluecolor

A CLI tool for **Linshang LS170** Bluetooth colorimeters. Likely works with LS171 just as well. Reverse engineered from the LScolor Android app.

Developed and tested on Linux but likely to work on Windows and macOS too. (let me know!)

Work in progress:

- [x] Scan for and connect to an appropriate device
- [x] Request and print color readings
- [x] Calibration
- [x] Battery status
- [ ] Parse device_info to something readable.

## Installation

First, install Rust and Cargo, e.g. using [Rustup](https://rustup.rs).

Then run:

    cargo install --git https://github.com/virtulis/bluecolor

## Usage

If run without any options (`bluecolor`), it will try to find an appropriate device among the paired ones and trigger a scan.

```
Usage: bluecolor [OPTIONS]

Options:
  -d, --device <DEVICE>              Address of the device to use (e.g. 00:11:22:33:44:55)
  -f, --format <FORMAT>              Output format (text, json) [default: text]
      --log-level <LOG_LEVEL>        Log level (error, warn, info, debug, trace)
      --scan-timeout <SCAN_TIMEOUT>  Timeout to find the device, in seconds [default: 5]
  -g, --get-status                   Get battery level and SN on launch
  -c, --calibrate                    Calibrate on launch (instead of the initial scan)
  -s, --scan                         Scan on launch
  -h, --help                         Print help
  -V, --version                      Print version
```

## Output example

### Text:

Run as:

    bluecolor --get-status --calibrate --scan

Output:

```
[2024-05-21T04:02:04Z INFO  bluecolor] Selected device: DC:8E:95:66:CD:B8 Some("LS170002377")
Update: device_info = [2023,2311,6921,-14053,1993,8711,2594,-13814,-15926,21953,85,28928,-11919,3793,14]
Update: power_level = 41
Update: calibrated = true
Scan result #: 1
	Lab: 92.58, -0.27, 0.54
	Luv: 92.58, -0.04, 0.87
	Lch: 92.58, 0.6, 116.51
	yxY: 82.03, 31.44, 33.21
	RGB: 234, 234, 231

```

### Text:

Run as:

    bluecolor --get-status --calibrate --scan --format json --log-level error

Output:

```ldjson
{"device_info":[2023,2311,6921,-14053,1993,8711,2594,-13814,-15926,21953,85,28928,-11919,3793,14]}
{"power_level":41}
{"calibrated":true}
{"scan":{"lab":[92.6,-0.29,0.59],"luv":[92.6,-0.04,0.94],"lch":[92.6,0.65,116.2],"yxy":[82.05,31.44,33.22],"rgb":[234,234,231]}}
{"scan":{"lab":[58.99,-12.03,21.17],"luv":[58.99,-5.33,29.22],"lch":[58.99,24.35,119.61],"yxy":[27.02,34.2,40.42],"rgb":[135,147,103]}}
{"scan":{"lab":[76.02,13.89,0.19],"luv":[76.02,20.25,-2.19],"lch":[76.02,13.9,0.82],"yxy":[49.93,33.69,32.04],"rgb":[213,179,186]}}
```

## Disclaimer

This project and author are not affiliated with or endorsed by Linshang in any way.

## License

MIT
