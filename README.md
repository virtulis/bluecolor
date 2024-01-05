# bluecolor

A CLI tool for **Linshang LS170** Bluetooth colorimeters. Likely works with LS171 just as well. Reverse engineered from the LScolor Android app.

Developed and tested on Linux but likely to work on Windows and macOS too. (let me know!)

Work in progress:

- [x] Scan for and connect to an appropriate device
- [x] Request and print color readings
- [ ] Calibration
- [ ] Battery status, S/N, etc.

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
  -h, --help                         Print help
  -V, --version                      Print version
```

## Disclaimer

This project and author are not affiliated with or endorsed by Linshang in any way.

## License

MIT
