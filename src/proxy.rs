use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::capture::CaptureLog;

struct ClientRelay {
    upstream: Arc<UdpSocket>,
}

pub fn run(bind: SocketAddr, upstream_addr: &str, capture: CaptureLog) -> io::Result<()> {
    let public = Arc::new(UdpSocket::bind(bind)?);
    public.set_read_timeout(Some(Duration::from_millis(250)))?;
    let upstream_target = upstream_addr.to_socket_addrs()?.next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "upstream did not resolve"))?;

    println!("[proxy] listening on {bind}, forwarding to {upstream_target}");
    println!("[proxy] point PROSPECT_SERVER_ADDRESS at {bind}");

    let mut relays: HashMap<SocketAddr, ClientRelay> = HashMap::new();
    let mut buf = [0u8; 65535];

    loop {
        match public.recv_from(&mut buf) {
            Ok((n, client)) => {
                capture.write("C>S", client, &buf[..n]);
                if !relays.contains_key(&client) {
                    let upstream = Arc::new(UdpSocket::bind("0.0.0.0:0")?);
                    upstream.connect(upstream_target)?;
                    upstream.set_read_timeout(Some(Duration::from_millis(500)))?;

                    let reader = upstream.clone();
                    let public_writer = public.clone();
                    let cap = capture.clone();
                    thread::spawn(move || {
                        let mut reply = [0u8; 65535];
                        loop {
                            match reader.recv(&mut reply) {
                                Ok(m) => {
                                    cap.write("S>C", client, &reply[..m]);
                                    if let Err(e) = public_writer.send_to(&reply[..m], client) {
                                        eprintln!("[proxy] send to {client} failed: {e}");
                                        break;
                                    }
                                }
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => continue,
                                Err(e) => {
                                    eprintln!("[proxy] upstream receive for {client} failed: {e}");
                                    break;
                                }
                            }
                        }
                    });
                    relays.insert(client, ClientRelay { upstream });
                    println!("[proxy] new client {client}");
                }
                if let Some(relay) = relays.get(&client) {
                    relay.upstream.send(&buf[..n])?;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e),
        }
    }
}
