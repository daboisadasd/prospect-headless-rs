#[derive(Debug, Clone)]
pub struct BitWriter {
    data: Vec<u8>,
    bit_len: usize,
}

impl BitWriter {
    pub fn new() -> Self {
        Self { data: Vec::new(), bit_len: 0 }
    }

    pub fn bit_len(&self) -> usize { self.bit_len }

    pub fn write_bit(&mut self, value: bool) {
        let byte_idx = self.bit_len >> 3;
        let bit_idx = self.bit_len & 7;
        if byte_idx == self.data.len() {
            self.data.push(0);
        }
        if value {
            self.data[byte_idx] |= 1u8 << bit_idx;
        }
        self.bit_len += 1;
    }

    pub fn write_bits_from_bytes(&mut self, src: &[u8], bit_count: usize) {
        assert!(bit_count <= src.len() * 8);
        for i in 0..bit_count {
            let bit = ((src[i >> 3] >> (i & 7)) & 1) != 0;
            self.write_bit(bit);
        }
    }

    pub fn write_bytes(&mut self, src: &[u8]) {
        self.write_bits_from_bytes(src, src.len() * 8);
    }

    pub fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    pub fn write_u16_le(&mut self, value: u16) {
        self.write_bytes(&value.to_le_bytes());
    }

    pub fn write_u32_le(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    pub fn write_i32_le(&mut self, value: i32) {
        self.write_bytes(&value.to_le_bytes());
    }

    pub fn write_f64_le(&mut self, value: f64) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// UE4 FString serialization for ASCII values: signed int32 count including NUL,
    /// followed by the bytes and trailing NUL. This is the form used for IPv4:port.
    pub fn write_fstring_ascii(&mut self, value: &str) {
        assert!(value.is_ascii(), "ASCII FString helper received non-ASCII text");
        if value.is_empty() {
            self.write_i32_le(0);
            return;
        }
        let count = value.len() + 1;
        self.write_i32_le(count as i32);
        self.write_bytes(value.as_bytes());
        self.write_bytes(&[0]);
    }

    pub fn finish_with_termination(mut self) -> Vec<u8> {
        self.write_bit(true);
        self.data
    }

    pub fn into_bytes(self) -> Vec<u8> { self.data }
}

#[derive(Debug, Clone, Copy)]
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_len: usize,
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8], bit_len: usize) -> Self {
        assert!(bit_len <= data.len() * 8);
        Self { data, bit_len, pos: 0 }
    }

    pub fn pos(&self) -> usize { self.pos }
    pub fn bits_left(&self) -> usize { self.bit_len.saturating_sub(self.pos) }

    pub fn read_bit(&mut self) -> Option<bool> {
        if self.pos >= self.bit_len { return None; }
        let v = ((self.data[self.pos >> 3] >> (self.pos & 7)) & 1) != 0;
        self.pos += 1;
        Some(v)
    }

    pub fn read_bits_to_vec(&mut self, bit_count: usize) -> Option<Vec<u8>> {
        if bit_count > self.bits_left() { return None; }
        let mut out = vec![0u8; (bit_count + 7) / 8];
        for i in 0..bit_count {
            if self.read_bit()? {
                out[i >> 3] |= 1u8 << (i & 7);
            }
        }
        Some(out)
    }

    pub fn read_bytes(&mut self, count: usize) -> Option<Vec<u8>> {
        self.read_bits_to_vec(count * 8)
    }

    pub fn read_u8(&mut self) -> Option<u8> {
        Some(self.read_bytes(1)?[0])
    }

    pub fn read_u16_le(&mut self) -> Option<u16> {
        let bytes = self.read_bytes(2)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_u32_le(&mut self) -> Option<u32> {
        let bytes = self.read_bytes(4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_i32_le(&mut self) -> Option<i32> {
        let bytes = self.read_bytes(4)?;
        Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_f64_le(&mut self) -> Option<f64> {
        let bytes = self.read_bytes(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(&bytes);
        Some(f64::from_le_bytes(a))
    }

    pub fn read_fstring_ascii(&mut self, max_len: usize) -> Option<String> {
        let count = self.read_i32_le()?;
        if count == 0 {
            return Some(String::new());
        }
        if count < 0 {
            // Negative FString lengths mean UTF-16/TCHAR serialization. The control
            // messages we have observed/ported so far use ASCII; reject it explicitly
            // instead of silently decoding the wrong wire shape.
            return None;
        }
        let count = count as usize;
        if count > max_len || count == 0 {
            return None;
        }
        let bytes = self.read_bytes(count)?;
        if bytes.last().copied() != Some(0) {
            return None;
        }
        if !bytes[..count - 1].is_ascii() {
            return None;
        }
        String::from_utf8(bytes[..count - 1].to_vec()).ok()
    }
}

/// UE packets append a single 1 termination bit then zero padding to the next byte.
/// Return the number of meaningful bits before that terminator.
pub fn payload_bit_len_from_termination(data: &[u8]) -> Option<usize> {
    let last = *data.last()?;
    if last == 0 { return None; }
    let highest_set = 7usize - last.leading_zeros() as usize;
    Some((data.len() - 1) * 8 + highest_set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsb_first_round_trip() {
        let mut w = BitWriter::new();
        w.write_bit(true);
        w.write_bit(false);
        w.write_bit(true);
        w.write_bytes(&[0xA5]);
        let bits = w.bit_len();
        let bytes = w.finish_with_termination();
        assert_eq!(payload_bit_len_from_termination(&bytes), Some(bits));
        let mut r = BitReader::new(&bytes, bits);
        assert_eq!(r.read_bit(), Some(true));
        assert_eq!(r.read_bit(), Some(false));
        assert_eq!(r.read_bit(), Some(true));
        assert_eq!(r.read_bytes(1).unwrap(), vec![0xA5]);
    }
}
