//! Arctis 7 / Pro and Arctis 9 protocols.

use crate::hid::{self, Hid, Protocol};
use std::io;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct Status {
    pub device: &'static str,
    pub product_id: u16,
    /// Headset powered on and paired with the receiver.
    pub powered: bool,
    /// Battery percentage, only meaningful when `powered`.
    pub battery: Option<u8>,
    pub charging: Option<bool>,
    /// 0 = full chat, 64 = centred, 127 = full game.
    pub chatmix: Option<u8>,
    pub version: Option<String>,
    /// Raw status report (hex), only for protocols not validated on hardware.
    pub raw: Option<String>,
}

pub fn read_status(hid: &mut Hid, dev: &hid::Found) -> io::Result<Status> {
    let mut st = Status {
        device: dev.model.name,
        product_id: dev.product_id,
        powered: false,
        battery: None,
        charging: None,
        chatmix: None,
        version: None,
        raw: None,
    };
    match dev.model.protocol {
        Protocol::Arctis7 => arctis7(hid, &mut st)?,
        Protocol::Arctis9 => arctis9(hid, &mut st)?,
    }
    Ok(st)
}

/// Arctis 7 / Pro: vendor commands `06 <cmd>`, answers echo the command.
/// Observed on an Arctis 7 (2019, 1038:12ad):
///   0x14 -> byte 2: 0x01 headset off (receiver only), 0x03 headset on
///   0x18 -> byte 2: battery percentage (0 when the headset is off)
///   0x24 -> bytes 2/3: game / chat mix (both 0 = centred)
///   0x10 -> bytes 2..: 13 01 13 01 (looks like a firmware/protocol version)
fn arctis7(hid: &mut Hid, st: &mut Status) -> io::Result<()> {
    let state = hid.query(&[0x06, 0x14], TIMEOUT)?;
    st.powered = state[2] == 0x03;

    if let Ok(v) = hid.query(&[0x06, 0x10], TIMEOUT) {
        st.version = Some(format!("{}.{}.{}.{}", v[2], v[3], v[4], v[5]));
    }
    if st.powered {
        let b = hid.query(&[0x06, 0x18], TIMEOUT)?;
        st.battery = Some(b[2].min(100));
        if let Ok(c) = hid.query(&[0x06, 0x24], TIMEOUT) {
            st.chatmix = Some(chatmix7(c[2], c[3]));
        }
    }
    Ok(())
}

/// Same mapping as HeadsetControl: game/chat are reported as separate values
/// in 191..=255; fold them into a single 0..=127 slider.
fn chatmix7(game: u8, chat: u8) -> u8 {
    let (game, chat) = (game as i32, chat as i32);
    let v = match (game, chat) {
        (0, 0) => 64,
        (0, c) => 64 + 255 - c,
        (g, _) => 64 - (255 - g),
    };
    v.clamp(0, 127) as u8
}

const A9_BATTERY_MIN: i32 = 0x64;
const A9_BATTERY_MAX: i32 = 0x9A;

/// Arctis 9: single status request `20 00`, 12-byte answer that does not echo
/// the command. Layout taken from HeadsetControl, NOT validated on hardware:
///   byte 3: raw battery (0x64..=0x9A), byte 4: 1 when charging,
///   byte 9: game volume (0..=19), byte 10: chat volume (0..=19).
/// There is no known "headset off" flag; a raw battery of 0 is used as the
/// off indicator (the receiver reports no battery when no headset is paired).
/// The raw report is exposed as `RAW` to make validation on a real unit easy.
fn arctis9(hid: &mut Hid, st: &mut Status) -> io::Result<()> {
    let r = hid.query_any(&[0x20, 0x00], TIMEOUT)?;
    st.raw = Some(r[..12].iter().map(|b| format!("{b:02x}")).collect());

    let raw_bat = r[3] as i32;
    st.powered = raw_bat != 0;
    if !st.powered {
        return Ok(());
    }
    st.charging = Some(r[4] == 0x01);
    let pct = (raw_bat - A9_BATTERY_MIN) * 100 / (A9_BATTERY_MAX - A9_BATTERY_MIN);
    st.battery = Some(pct.clamp(0, 100) as u8);

    // HeadsetControl: 64 - (map(b9, 0..19 -> 0..64) + map(b10, 0..19 -> 0..-64))
    let game = r[9] as i32 * 64 / 19;
    let chat = r[10] as i32 * 64 / 19;
    st.chatmix = Some((64 - game + chat).clamp(0, 127) as u8);
    Ok(())
}
