mod bit;
mod capture;
mod control;
mod handshake;
mod proxy;
mod session;
mod sha1;

use std::env;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::str::FromStr;
use std::time::Duration;

use capture::{hex, CaptureLog};
use handshake::{Incoming, StatelessHandshakeServer};

#[derive(Debug)]
struct Args {
    mode: String,
    bind: SocketAddr,
    upstream: Option<String>,
    capture: String,
}

fn main() -> io::Result<()> {
    let args = parse_args()?;
    let capture = CaptureLog::open(&args.capture)?;

    match args.mode.as_str() {
        "proxy" => {
            let upstream = args.upstream.as_deref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "--mode proxy requires --upstream ip:port")
            })?;
            proxy::run(args.bind, upstream, capture)
        }
        "handshake" => run_handshake(args.bind, capture),
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "mode must be proxy or handshake")),
    }
}

fn run_handshake(bind: SocketAddr, capture: CaptureLog) -> io::Result<()> {
    let socket = UdpSocket::bind(bind)?;
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;
    let mut hs = StatelessHandshakeServer::new()?;
    let mut buf = [0u8; 65535];

    println!("[handshake] listening on {bind}");
    println!("[handshake] this milestone intentionally stops after UE4's stateless UDP handshake");
    println!("[handshake] once a client sends its first post-handshake packet, it is captured for the NMT layer");

    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, addr)) => {
                let data = &buf[..n];
                capture.write("C>S", addr, data);
                match hs.handle(addr, data) {
                    Incoming::Send(reply) => {
                        println!("[handshake] challenge -> {addr} ({} bytes)", reply.len());
                        capture.write("S>C", addr, &reply);
                        socket.send_to(&reply, addr)?;
                    }
                    Incoming::Established(seed, reply) => {
                        println!(
                            "[handshake] ESTABLISHED {addr}; server_seq={} client_seq={} cookie_ack={}",
                            seed.server_sequence,
                            seed.client_sequence,
                            hex(&reply)
                        );
                        capture.write("S>C", addr, &reply);
                        socket.send_to(&reply, addr)?;
                    }
                    Incoming::Data(seed) => {
                        println!(
                            "[handshake] post-handshake data from {addr}: {} bytes; seeds s={} c={}. Next milestone is packet/bunch + NMT_Hello.",
                            data.len(), seed.server_sequence, seed.client_sequence
                        );
                    }
                    Incoming::Ignore(why) => {
                        eprintln!("[handshake] ignored {addr} {} bytes: {why}", data.len());
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e),
        }
    }
}

fn parse_args() -> io::Result<Args> {
    let mut mode = "handshake".to_string();
    let mut bind = SocketAddr::from_str("0.0.0.0:7777").unwrap();
    let mut upstream = None;
    let mut capture = "prospect-wire.log".to_string();

    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--mode" => mode = it.next().ok_or_else(|| missing("--mode value"))?,
            "--bind" => {
                let v = it.next().ok_or_else(|| missing("--bind value"))?;
                bind = SocketAddr::from_str(&v).map_err(|_| missing("invalid --bind ip:port"))?;
            }
            "--upstream" => upstream = Some(it.next().ok_or_else(|| missing("--upstream value"))?),
            "--capture" => capture = it.next().ok_or_else(|| missing("--capture value"))?,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(missing(&format!("unknown argument: {other}"))),
        }
    }

    Ok(Args { mode, bind, upstream, capture })
}

fn missing(msg: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidInput, msg.to_string()) }

fn print_help() {
    println!("prospect-headless 0.1.0");
    println!("\nModes:");
    println!("  handshake  Standalone UE4 StatelessConnect handshake server (default)");
    println!("  proxy      Transparent UDP relay to a working ?listen host, with exact wire capture");
    println!("\nOptions:");
    println!("  --bind IP:PORT       default 0.0.0.0:7777");
    println!("  --upstream IP:PORT   required in proxy mode");
    println!("  --capture PATH       default prospect-wire.log");
}
