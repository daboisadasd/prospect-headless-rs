use std::collections::HashMap;
#[cfg(unix)]
use std::fs::File;
use std::io;
#[cfg(unix)]
use std::io::Read;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::bit::{payload_bit_len_from_termination, BitReader, BitWriter};
use crate::sha1::hmac_sha1;

pub const COOKIE_BYTES: usize = 20;
pub const SECRET_BYTES: usize = 64;
pub const HANDSHAKE_PACKET_BITS: usize = 227;
pub const RESTART_PACKET_BITS: usize = 2;
pub const RESTART_RESPONSE_BITS: usize = 387;
pub const MAX_PACKET_ID: u16 = 16384;

#[derive(Debug, Clone, Copy)]
pub struct ConnectionSeed {
    pub server_sequence: u16,
    pub client_sequence: u16,
    pub established_at: Instant,
}

#[derive(Debug)]
pub enum Incoming {
    Send(Vec<u8>),
    Established(ConnectionSeed, Vec<u8>),
    Data(ConnectionSeed),
    Ignore(&'static str),
}

pub struct StatelessHandshakeServer {
    started: Instant,
    last_secret_rotation: Instant,
    active_secret: usize,
    secrets: [[u8; SECRET_BYTES]; 2],
    established: HashMap<SocketAddr, ConnectionSeed>,
}

impl StatelessHandshakeServer {
    pub fn new() -> io::Result<Self> {
        let now = Instant::now();
        let mut secrets = [[0u8; SECRET_BYTES]; 2];
        os_random_fill(&mut secrets[0])?;
        os_random_fill(&mut secrets[1])?;
        Ok(Self {
            started: now,
            last_secret_rotation: now,
            active_secret: 0,
            secrets,
            established: HashMap::new(),
        })
    }

    pub fn established_seed(&self, addr: &SocketAddr) -> Option<ConnectionSeed> {
        self.established.get(addr).copied()
    }

    pub fn handle(&mut self, addr: SocketAddr, datagram: &[u8]) -> Incoming {
        self.maybe_rotate_secret();

        let Some(payload_bits) = payload_bit_len_from_termination(datagram) else {
            return Incoming::Ignore("missing UE termination bit");
        };
        let mut r = BitReader::new(datagram, payload_bits);
        let Some(is_handshake) = r.read_bit() else {
            return Incoming::Ignore("empty packet");
        };

        if !is_handshake {
            if let Some(seed) = self.established.get(&addr).copied() {
                return Incoming::Data(seed);
            }
            return Incoming::Ignore("data packet before stateless handshake completed");
        }

        if payload_bits == RESTART_PACKET_BITS {
            let restart = r.read_bit().unwrap_or(false);
            return if restart {
                Incoming::Ignore("client sent restart-handshake request; not needed for normal local testing")
            } else {
                Incoming::Ignore("invalid 2-bit handshake packet")
            };
        }

        if payload_bits != HANDSHAKE_PACKET_BITS && payload_bits != RESTART_RESPONSE_BITS {
            return Incoming::Ignore("unexpected UE4 handshake bit length");
        }

        let restart = match r.read_bit() { Some(v) => v, None => return Incoming::Ignore("truncated restart flag") };
        let secret_id = match r.read_bit() { Some(v) => v as usize, None => return Incoming::Ignore("truncated secret id") };
        let timestamp = match r.read_f64_le() { Some(v) => v, None => return Incoming::Ignore("truncated timestamp") };
        let cookie_vec = match r.read_bytes(COOKIE_BYTES) { Some(v) => v, None => return Incoming::Ignore("truncated cookie") };
        let mut cookie = [0u8; COOKIE_BYTES];
        cookie.copy_from_slice(&cookie_vec);

        if restart && payload_bits == RESTART_RESPONSE_BITS {
            // We can add NAT rebinding/restart support after the normal connection path is proven.
            return Incoming::Ignore("restart response parsed but NAT-rebind support is not implemented yet");
        }

        if timestamp == 0.0 {
            let challenge = self.make_challenge(addr);
            return Incoming::Send(challenge);
        }

        if timestamp < 0.0 {
            return Incoming::Ignore("client sent challenge ACK; ACK is server-to-client in this role");
        }

        let now_elapsed = self.elapsed_seconds();
        let age = now_elapsed - timestamp;
        if !(0.0..40.0).contains(&age) {
            return Incoming::Ignore("challenge cookie expired or timestamp is from the future");
        }

        if secret_id > 1 {
            return Incoming::Ignore("invalid secret id");
        }
        let expected = self.generate_cookie(addr, secret_id, timestamp);
        if !constant_time_eq(&cookie, &expected) {
            return Incoming::Ignore("invalid challenge cookie");
        }

        let server_sequence = u16::from_le_bytes([cookie[0], cookie[1]]) & (MAX_PACKET_ID - 1);
        let client_sequence = u16::from_le_bytes([cookie[2], cookie[3]]) & (MAX_PACKET_ID - 1);
        let seed = ConnectionSeed { server_sequence, client_sequence, established_at: Instant::now() };
        self.established.insert(addr, seed);

        let ack = make_handshake_packet(false, true, -1.0, &cookie);
        Incoming::Established(seed, ack)
    }

    fn elapsed_seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64().max(0.000_001)
    }

    fn make_challenge(&self, addr: SocketAddr) -> Vec<u8> {
        let timestamp = self.elapsed_seconds();
        let secret_id = self.active_secret;
        let cookie = self.generate_cookie(addr, secret_id, timestamp);
        make_handshake_packet(false, secret_id != 0, timestamp, &cookie)
    }

    fn generate_cookie(&self, addr: SocketAddr, secret_id: usize, timestamp: f64) -> [u8; COOKIE_BYTES] {
        let endpoint = addr.to_string();
        let mut w = BitWriter::new();
        w.write_f64_le(timestamp);
        w.write_fstring_ascii(&endpoint);
        hmac_sha1(&self.secrets[secret_id], &w.into_bytes())
    }

    fn maybe_rotate_secret(&mut self) {
        // UE4 rotates frequently and keeps two secrets. A fixed 15s interval is enough
        // for compatibility; both current and previous secrets remain available.
        if self.last_secret_rotation.elapsed() >= Duration::from_secs(15) {
            self.active_secret ^= 1;
            if os_random_fill(&mut self.secrets[self.active_secret]).is_ok() {
                self.last_secret_rotation = Instant::now();
            }
        }
    }
}

fn make_handshake_packet(restart: bool, secret_id: bool, timestamp: f64, cookie: &[u8; COOKIE_BYTES]) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_bit(true); // bHandshakePacket
    w.write_bit(restart);
    w.write_bit(secret_id);
    w.write_f64_le(timestamp);
    w.write_bytes(cookie);
    debug_assert_eq!(w.bit_len(), HANDSHAKE_PACKET_BITS);
    w.finish_with_termination()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b) { diff |= x ^ y; }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn prospect_handshake_reference_vector() {
        let mut secret = [0u8; SECRET_BYTES];
        for (i, b) in secret.iter_mut().enumerate() {
            *b = i as u8;
        }

        let addr: SocketAddr = "127.0.0.1:54321".parse().unwrap();
        let timestamp = 12.5f64;
        let mut cookie_input = BitWriter::new();
        cookie_input.write_f64_le(timestamp);
        cookie_input.write_fstring_ascii(&addr.to_string());
        let cookie = hmac_sha1(&secret, &cookie_input.into_bytes());

        assert_eq!(
            hex(&cookie),
            "996dba28fbfeddc79db26387e22b654eb561f1de"
        );

        let packet = make_handshake_packet(false, false, timestamp, &cookie);
        assert_eq!(packet.len(), 29);
        assert_eq!(payload_bit_len_from_termination(&packet), Some(HANDSHAKE_PACKET_BITS));
        assert_eq!(
            hex(&packet),
            "0100000000004801ca6cd345d9f7ef3eee941d3b145f2973aa0d8bf70e"
        );
    }
}

#[cfg(unix)]
fn os_random_fill(buf: &mut [u8]) -> io::Result<()> {
    File::open("/dev/urandom")?.read_exact(buf)
}

#[cfg(windows)]
type NtStatus = i32;

#[cfg(windows)]
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptGenRandom(
        h_algorithm: *mut core::ffi::c_void,
        buffer: *mut u8,
        count: u32,
        flags: u32,
    ) -> NtStatus;
}

#[cfg(windows)]
fn os_random_fill(buf: &mut [u8]) -> io::Result<()> {
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    let count = u32::try_from(buf.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "RNG request too large"))?;
    let status = unsafe {
        BCryptGenRandom(
            core::ptr::null_mut(),
            buf.as_mut_ptr(),
            count,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status >= 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("BCryptGenRandom NTSTATUS=0x{:08x}", status as u32),
        ))
    }
}
