# bluecolor

A CLI tool for **Linshang LS170** and **LS171** Bluetooth colorimeters. Reverse engineered from the LScolor Android app.

Developed and tested on Linux but likely to work on Windows and macOS too. (let me know!)

Work in progress:

- [x] Scan for and connect to an appropriate device
- [x] Request and print color readings
- [x] Calibration
- [x] Battery status
- [ ] Parse device_info to something readable.

There is a basic [web-based UI](https://github.com/virtulis/dabadee) available for scanning charts in batches.

## Changes

### 0.3.0

* Breaking: changed JSON output format.
* Added WebSocket server mode and TUI "interactive" mode.

## Installation

First, install Rust and Cargo, e.g. using [Rustup](https://rustup.rs).

Then run:

    cargo install --git https://github.com/virtulis/bluecolor

## Usage

If run without any options (`bluecolor`), it will try to find an appropriate device among the paired ones and trigger a scan.

```
Usage: bluecolor [OPTIONS]

Options:
  -d, --device <DEVICE>
          Address of the device to use (e.g. 00:11:22:33:44:55)
  -f, --format <FORMAT>
          Output format (text, json) [default: text]
      --pipe
          Skip checking for TTY and always run non-interactive
      --log-level <LOG_LEVEL>
          Log level (error, warn, info, debug, trace)
      --find-timeout <FIND_TIMEOUT>
          Timeout to find the device, in seconds [default: 10]
      --remain
          Do not exit on disconnect or error
      --connect-timeout <CONNECT_TIMEOUT>
          Assume connect attempt failed if there is no result for that many seconds [default: 30]
      --reconnect-attempts <RECONNECT_ATTEMPTS>
          Reconnect attempts (if all fail, give up until new commands are received) [default: 10]
      --reconnect-interval <RECONNECT_INTERVAL>
          Seconds to wait between reconnect attempts [default: 30]
      --keepalive-interval <KEEPALIVE_INTERVAL>
          Send status command if connected but idle for that many seconds [default: 30]
  -g, --get-status
          Get battery level and SN on launch
  -c, --calibrate
          Calibrate on launch (instead of the initial scan)
  -s, --scan
          Scan on launch
      --listen <PORT>
          Start a multi-tenant WebSocket server on this port
      --host <HOST>
          Websocket server host [default: 127.0.0.1]
  -h, --help
          Print help
  -V, --version
          Print version

```

## Troubleshooting

### Color accuracy

* LS171 is significantly more accurate on most surfaces.
* These colorimeters report Lab values with the **D65** illuminant. Most color profiling software (e.g. Argyll) expects **D50**. If your hues are off, make sure you convert readings to the appropriate illuminant.
* It takes several seconds for a sample to scan, avoid moving the device until you receive the reading.

### Connection

* The device *requires* Bluetooth LE and will not work without it.
* The connection can be rather flaky (especially on first-gen? LS170). It may help to:
  * Disable Wi-Fi and any other BT devices on the receiving device. 
  * Move the colorimeter closer to the receiving device.
* If you have trouble initiating the connection, ensure the adapter is scanning for devices *before* starting bluecolor. E.g.
  * E.g. `bluetoothctl scan le`

## Usage examples

### Interactive

A very basic "interactive" mode is exposed by default (if run in a terminal)

    bluecolor

Available commands are

    status
    calibrate
    scan
    disconnect
    reconnect
    exit

There are no arguments to any of these.

### Server mode

Runs a persistent WebSocket server that can receive commands and will broadcast results to all clients. Used by [dabadee](https://github.com/virtulis/dabadee). No encryption or auth is available.

    bluecolor --listen 8765

Commands are same as above, sent as JSON arrays with one element, e.g. `["scan"]`.

Responses are tuples of `["command", ...results]` (see JSON mode below).

### Non-interactive, text:

Run as:

    bluecolor --pipe --get-status --calibrate --scan

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

### Non-interactive, JSON:

Run as:

    bluecolor --pipe --get-status --calibrate --scan --format json --log-level error

Output:

```json lines
["connecting",null,null]
["connected","DC:8E:95:66:CD:B8","LS170002377"]
["device_info",[2023,2311,6921,-14053,1993,8711,2594,-13814,-15926,21953,85,28928,-11919,3793,14]]
["power_level",41]
["calibrated"]
["scan",1,{"scan":{"lab":[92.59,-0.29,0.55],"luv":[92.59,-0.07,0.89],"lch":[92.59,0.62,117.89],"yxy":[82.05,31.43,33.22],"rgb":[234,234,231]}}]
```

## Disclaimer

This project and author are not affiliated with or endorsed by Linshang in any way.

## License

MIT
