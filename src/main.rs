mod headset;
mod hid;

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_LISTEN: &str = "127.0.0.1:7878";
const CACHE_TTL: Duration = Duration::from_millis(1000);

struct State {
    cache: Option<(Instant, String)>,
}

fn status_text() -> String {
    let Some(dev) = hid::find_device() else {
        return "RECEIVER: absent\nPOWER: off\n".into();
    };
    let result = hid::Hid::open(&dev.path).and_then(|mut h| headset::read_status(&mut h, &dev));
    match result {
        Ok(s) => {
            let mut out = format!(
                "RECEIVER: present\nDEVICE: {}\nPRODUCT_ID: {:04x}\nPOWER: {}\n",
                s.device,
                s.product_id,
                if s.powered { "on" } else { "off" }
            );
            if let Some(b) = s.battery {
                out += &format!("BATTERY: {b}\n");
            }
            if let Some(c) = s.charging {
                out += &format!("CHARGING: {}\n", if c { "yes" } else { "no" });
            }
            if let Some(c) = s.chatmix {
                out += &format!("CHATMIX: {c}\n");
            }
            if let Some(v) = s.version {
                out += &format!("VERSION: {v}\n");
            }
            if let Some(r) = s.raw {
                out += &format!("RAW: {r}\n");
            }
            out
        }
        Err(e) => format!("RECEIVER: error\nERROR: {e}\nPOWER: unknown\n"),
    }
}

fn cached_status(state: &Mutex<State>) -> String {
    let mut st = state.lock().unwrap();
    if let Some((t, s)) = &st.cache {
        if t.elapsed() < CACHE_TTL {
            return s.clone();
        }
    }
    let s = status_text();
    st.cache = Some((Instant::now(), s.clone()));
    s
}

fn handle(stream: TcpStream, state: Arc<Mutex<State>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let Ok(mut out) = stream.try_clone() else { return };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        let reply = match line.trim().to_ascii_uppercase().as_str() {
            "" => continue,
            "STATUS" => format!("OK\n{}\n", cached_status(&state)),
            "HELP" => "OK\nCommands: STATUS, HELP, QUIT\n\n".into(),
            "QUIT" => break,
            _ => "ERR unknown command (try HELP)\n\n".into(),
        };
        if out.write_all(reply.as_bytes()).is_err() {
            break;
        }
    }
}

fn main() {
    let mut listen = DEFAULT_LISTEN.to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-l" | "--listen" => listen = args.next().unwrap_or_else(|| usage()),
            "--once" => {
                print!("{}", status_text());
                return;
            }
            _ => usage(),
        }
    }

    let listener = TcpListener::bind(&listen).unwrap_or_else(|e| {
        eprintln!("cannot bind {listen}: {e}");
        exit(1);
    });
    eprintln!("arctis-rs listening on {listen}");
    let state = Arc::new(Mutex::new(State { cache: None }));
    for conn in listener.incoming().flatten() {
        let state = Arc::clone(&state);
        thread::spawn(move || handle(conn, state));
    }
}

fn usage() -> ! {
    eprintln!("usage: arctis-rs [--listen ADDR:PORT] [--once]\n  default listen: {DEFAULT_LISTEN}\n  --once: print the status once and exit");
    exit(2);
}
