//! Minimal hidraw access (no libhidapi): locate the Arctis control interface
//! through sysfs and exchange 31-byte reports with it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

const O_NONBLOCK: i32 = 0o4000;
const REPORT_LEN: usize = 31;

pub const VENDOR_STEELSERIES: u16 = 0x1038;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Arctis7,
    Arctis9,
}

/// A supported model family.
pub struct Model {
    pub name: &'static str,
    pub protocol: Protocol,
    pub product_ids: &'static [u16],
    /// USB interface carrying the vendor control endpoint.
    pub interface: u8,
}

pub const MODELS: &[Model] = &[
    Model {
        name: "SteelSeries Arctis 7 / Pro",
        protocol: Protocol::Arctis7,
        // 0x1260: Arctis 7 (2017), 0x12ad: Arctis 7 (2019), 0x1252: Arctis Pro (2019)
        product_ids: &[0x1260, 0x12ad, 0x1252],
        interface: 5,
    },
    Model {
        name: "SteelSeries Arctis 9",
        protocol: Protocol::Arctis9,
        product_ids: &[0x12c2],
        interface: 0,
    },
];

pub struct Found {
    pub model: &'static Model,
    pub product_id: u16,
    pub path: PathBuf,
}

/// Scan /sys/class/hidraw for a supported receiver.
pub fn find_device() -> Option<Found> {
    let mut entries: Vec<_> = fs::read_dir("/sys/class/hidraw").ok()?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let dev = entry.path().join("device");
        let Ok(uevent) = fs::read_to_string(dev.join("uevent")) else { continue };
        let Some((vid, pid)) = parse_hid_id(&uevent) else { continue };
        if vid != VENDOR_STEELSERIES {
            continue;
        }
        let Some(model) = MODELS.iter().find(|m| m.product_ids.contains(&pid)) else { continue };
        // <hid dev>/../bInterfaceNumber
        let iface = fs::read_to_string(dev.join("../bInterfaceNumber")).ok();
        let iface = iface.and_then(|s| u8::from_str_radix(s.trim(), 16).ok());
        if iface != Some(model.interface) {
            continue;
        }
        return Some(Found {
            model,
            product_id: pid,
            path: Path::new("/dev").join(entry.file_name()),
        });
    }
    None
}

fn parse_hid_id(uevent: &str) -> Option<(u16, u16)> {
    let line = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut it = line.split(':');
    it.next()?; // bus
    let vid = u32::from_str_radix(it.next()?, 16).ok()?;
    let pid = u32::from_str_radix(it.next()?, 16).ok()?;
    Some((vid as u16, pid as u16))
}

pub struct Hid {
    file: File,
}

impl Hid {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(O_NONBLOCK)
            .open(path)?;
        Ok(Self { file })
    }

    /// Send a command and wait for the report echoing it (unrelated reports,
    /// e.g. volume events, are discarded).
    pub fn query(&mut self, cmd: &[u8], timeout: Duration) -> io::Result<[u8; REPORT_LEN]> {
        self.query_inner(cmd, timeout, true)
    }

    /// Like `query`, but accept the first report received (devices whose
    /// answers do not echo the command, such as the Arctis 9).
    pub fn query_any(&mut self, cmd: &[u8], timeout: Duration) -> io::Result<[u8; REPORT_LEN]> {
        self.query_inner(cmd, timeout, false)
    }

    fn query_inner(
        &mut self,
        cmd: &[u8],
        timeout: Duration,
        echo: bool,
    ) -> io::Result<[u8; REPORT_LEN]> {
        self.drain();
        let mut out = [0u8; REPORT_LEN];
        out[..cmd.len()].copy_from_slice(cmd);
        self.file.write_all(&out)?;

        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            match self.file.read(&mut buf) {
                Ok(n) if n >= cmd.len() && (!echo || buf[..cmd.len()] == *cmd) => {
                    let mut r = [0u8; REPORT_LEN];
                    let n = n.min(REPORT_LEN);
                    r[..n].copy_from_slice(&buf[..n]);
                    return Ok(r);
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => sleep(Duration::from_millis(5)),
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "no answer from receiver"))
    }

    fn drain(&mut self) {
        let mut buf = [0u8; 64];
        while matches!(self.file.read(&mut buf), Ok(n) if n > 0) {}
    }
}
