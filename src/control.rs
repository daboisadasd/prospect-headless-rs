//! UE4 control-channel message payload codecs.
//!
//! These are the message IDs/types used by the public Prospect.Unreal port for
//! this client generation. They are payload codecs only; reliable control-bunch
//! framing is intentionally a separate layer.

use crate::bit::{BitReader, BitWriter};

pub const NMT_HELLO: u8 = 0;
pub const NMT_WELCOME: u8 = 1;
pub const NMT_UPGRADE: u8 = 2;
pub const NMT_CHALLENGE: u8 = 3;
pub const NMT_NETSPEED: u8 = 4;
pub const NMT_LOGIN: u8 = 5;
pub const NMT_FAILURE: u8 = 6;
pub const NMT_JOIN: u8 = 9;
pub const NMT_JOIN_SPLIT: u8 = 10;
pub const NMT_SKIP: u8 = 12;
pub const NMT_ABORT: u8 = 13;
pub const NMT_PC_SWAP: u8 = 15;
pub const NMT_ACTOR_CHANNEL_FAILURE: u8 = 16;
pub const NMT_DEBUG_TEXT: u8 = 17;
pub const NMT_NET_GUID_ASSIGN: u8 = 18;
pub const NMT_SECURITY_VIOLATION: u8 = 19;
pub const NMT_GAME_SPECIFIC: u8 = 20;
pub const NMT_ENCRYPTION_ACK: u8 = 21;
pub const NMT_DESTRUCTION_INFO: u8 = 22;
pub const NMT_BEACON_WELCOME: u8 = 25;
pub const NMT_BEACON_JOIN: u8 = 26;
pub const NMT_BEACON_ASSIGN_GUID: u8 = 27;
pub const NMT_BEACON_NET_GUID_ACK: u8 = 28;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub is_little_endian: u8,
    pub remote_network_version: u32,
    pub encryption_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Welcome<'a> {
    pub map: &'a str,
    pub game_mode: &'a str,
    pub redirect_url: &'a str,
}

pub fn parse_hello_payload(reader: &mut BitReader<'_>) -> Option<Hello> {
    let is_little_endian = reader.read_u8()?;
    let remote_network_version = reader.read_u32_le()?;
    let encryption_token = reader.read_fstring_ascii(64 * 1024)?;
    Some(Hello { is_little_endian, remote_network_version, encryption_token })
}

pub fn write_challenge_message(writer: &mut BitWriter, challenge: &str) {
    writer.write_u8(NMT_CHALLENGE);
    writer.write_fstring_ascii(challenge);
}

pub fn write_welcome_message(writer: &mut BitWriter, welcome: &Welcome<'_>) {
    writer.write_u8(NMT_WELCOME);
    // UE: LevelName, GameName, RedirectURL.
    writer.write_fstring_ascii(welcome.map);
    writer.write_fstring_ascii(welcome.game_mode);
    writer.write_fstring_ascii(welcome.redirect_url);
}

pub fn parse_netspeed_payload(reader: &mut BitReader<'_>) -> Option<i32> {
    reader.read_i32_le()
}

pub fn parse_join_payload(reader: &mut BitReader<'_>) -> Option<()> {
    // NMT_Join has no parameters. It is valid only when the enclosing bunch
    // contains no remaining message payload bits for this control message.
    if reader.bits_left() == 0 { Some(()) } else { None }
}

pub fn write_failure_message(writer: &mut BitWriter, reason: &str) {
    writer.write_u8(NMT_FAILURE);
    writer.write_fstring_ascii(reason);
}

pub fn write_upgrade_message(writer: &mut BitWriter, network_version: u32) {
    writer.write_u8(NMT_UPGRADE);
    writer.write_u32_le(network_version);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_payload_round_trip_shape() {
        let mut w = BitWriter::new();
        w.write_u8(1);
        w.write_u32_le(0x1234_5678);
        w.write_fstring_ascii("");
        let bits = w.bit_len();
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes, bits);
        assert_eq!(
            parse_hello_payload(&mut r),
            Some(Hello {
                is_little_endian: 1,
                remote_network_version: 0x1234_5678,
                encryption_token: String::new(),
            })
        );
        assert_eq!(r.bits_left(), 0);
    }

    #[test]
    fn challenge_message_shape() {
        let mut w = BitWriter::new();
        write_challenge_message(&mut w, "0");
        assert_eq!(w.into_bytes(), vec![NMT_CHALLENGE, 2, 0, 0, 0, b'0', 0]);
    }

    #[test]
    fn welcome_field_order_is_map_game_mode_redirect() {
        let mut w = BitWriter::new();
        write_welcome_message(&mut w, &Welcome {
            map: "Map",
            game_mode: "Mode",
            redirect_url: "",
        });
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes, bytes.len() * 8);
        assert_eq!(r.read_u8(), Some(NMT_WELCOME));
        assert_eq!(r.read_fstring_ascii(1024).as_deref(), Some("Map"));
        assert_eq!(r.read_fstring_ascii(1024).as_deref(), Some("Mode"));
        assert_eq!(r.read_fstring_ascii(1024).as_deref(), Some(""));
        assert_eq!(r.bits_left(), 0);
    }

    #[test]
    fn join_has_no_payload() {
        let bytes: [u8; 0] = [];
        let mut r = BitReader::new(&bytes, 0);
        assert_eq!(parse_join_payload(&mut r), Some(()));
    }
}
