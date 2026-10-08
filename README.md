# arctis-rs

Small Rust daemon (std only, no dependencies, no libhidapi) that queries a
SteelSeries Arctis headset through `/dev/hidraw*` and exposes its state over
plain-text TCP.

Supported models: Arctis 7 (2017 `1260`, 2019 `12ad`), Arctis Pro 2019 (`1252`)
and Arctis 9 (`12c2`). The protocol is taken from
[HeadsetControl](https://github.com/Sapd/HeadsetControl) and was verified on an
Arctis 7 2019. **Arctis 9 support has not been tested on hardware** (see below).

## Desktop widget

A native KDE Plasma 6 widget showing power state and battery level (coloured
by level) is available in [arctis-tray](https://github.com/asyd/arctis-tray).

## Usage

```sh
cargo build --release
./target/release/arctis-rs                       # listens on 127.0.0.1:7878
./target/release/arctis-rs --listen 0.0.0.0:9000
./target/release/arctis-rs --once                # print the status and exit
```

```
$ echo STATUS | nc -q1 127.0.0.1 7878
OK
RECEIVER: present
DEVICE: SteelSeries Arctis 7 / Pro
PRODUCT_ID: 12ad
POWER: on
BATTERY: 28
CHATMIX: 64
VERSION: 19.1.19.1

```

Commands: `STATUS`, `HELP`, `QUIT`. A reply is `OK`, then `KEY: value` lines,
then an empty line; errors are `ERR ...`. Several commands can be sent on one
connection. The result is cached for 1 s.

| Key | Meaning |
|---|---|
| `RECEIVER` | `present`, `absent` or `error` (USB receiver plugged in or not) |
| `POWER` | `on` if the **headset** is powered on and paired, `off` if it is off (receiver only), `unknown` on error |
| `BATTERY` | level in % (only when the headset is on) |
| `CHATMIX` | 0 = chat, 64 = centred, 127 = game (ChatMix wheel) |
| `VERSION` | version bytes returned by command `0x10` (meaning not confirmed) |

Extra keys (Arctis 9 only): `CHARGING: yes|no` and `RAW: <hex>` (raw report).

### Arctis 9 (untested on hardware)

The protocol (request `20 00`, 12-byte reply: byte 3 = raw battery
`0x64..0x9A`, byte 4 = charging, bytes 9/10 = game/chat volume) comes from
HeadsetControl. There is no known "headset off" flag: the daemon treats the
headset as off when the battery byte is 0, which is an **assumption**. To
validate it, compare `RAW` with the headset on and off (`arctis-rs --once`) and
adjust `arctis9()` in `src/headset.rs`. The ChatMix formula is HeadsetControl's;
its direction (0 = chat or game) is not verified.

### How "powered on" is detected (Arctis 7)

Command `06 14` returns `01` in byte 2 when only the receiver is present and
`03` when the headset is on. The battery (`06 18`) is `0` when the headset is
off. Both were observed by switching the headset on and off.

### Other possible information

- Already exposed: power, battery, ChatMix, version.
- Possible but write operations (not exposed, the daemon is read-only):
  sidetone (`06 35`), inactivity timeout (`06 51`), LEDs (`06 55`).
- The receiver also sends unsolicited reports (volume, mic mute, etc.): a
  push-style "events" mode would be possible.
- No voltage reading or charging state on the Arctis 7 (the Arctis 9 does
  report charging state and voltage).

## Permissions: udev rule for the `plugdev` group

By default the hidraw node is owned by `root`, so a udev rule is needed to open
it as a regular user.

1. Install the rule:

   ```sh
   sudo cp udev/70-arctis.rules /etc/udev/rules.d/70-arctis.rules
   sudo udevadm control --reload-rules
   sudo udevadm trigger --subsystem-match=hidraw --action=add
   ```

   (or unplug and replug the receiver).

2. Make sure the user running the daemon is in the `plugdev` group (`id`,
   otherwise `sudo usermod -aG plugdev $USER` and log in again).

3. Check:

   ```sh
   ls -l /dev/hidraw*     # the receiver's node should be root:plugdev 660
   ```

The rule matches vendor `1038` and product ids `1260`, `12ad`, `1252`, `12c2`.
hidraw node numbers are not stable, hence the match on USB attributes rather
than on `/dev/hidrawN`.

Note: on a machine with seat/logind, a `uaccess` tag may already grant access
to the logged-in user; the rule above is meant for daemons and remote
sessions.

## Installation (binary + udev rule + systemd service)

```sh
make build                         # build as your user (install skips cargo if the binary exists)
sudo make install                  # PREFIX=/usr/local by default
sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=hidraw --action=add
sudo systemctl daemon-reload
sudo systemctl enable --now arctis-rs
```

The service runs as a `DynamicUser` with the supplementary group `plugdev` (hence
the udev rule) and systemd hardening; it listens on `127.0.0.1:7878`. To change
the address, run `systemctl edit arctis-rs` and override `ExecStart=`.
`make uninstall` removes the three installed files.

## Security

The daemon is read-only (no write command is ever sent to the headset) and
listens on loopback only by default.

## License

GPL-3.0-or-later, see [LICENSE](LICENSE).
