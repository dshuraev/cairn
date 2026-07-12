//! Canonical encoding primitives (§4.1).
//!
//! Every object's on-disk/hashed representation is built from these primitives so
//! that two semantically identical objects always produce byte-identical output.

use crate::hash::Hash;

/// Accumulates bytes for an object's canonical encoding.
#[derive(Debug, Default)]
pub struct Encoder(Vec<u8>);

impl Encoder {
    /// Creates an empty encoder.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Writes a single byte (§4.3 `kind_tag` / `has_link_group` fields).
    pub fn write_u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }

    /// Writes a little-endian `u32`.
    pub fn write_u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// Writes a `u32` length prefix followed by the UTF-8 bytes of `s`, with no
    /// terminator.
    pub fn write_str(&mut self, s: &str) -> &mut Self {
        self.write_u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
        self
    }

    /// Writes a `u32` length prefix followed by the raw bytes of `b`.
    pub fn write_bytes(&mut self, b: &[u8]) -> &mut Self {
        self.write_u32(b.len() as u32);
        self.0.extend_from_slice(b);
        self
    }

    /// Writes the raw 32 bytes of `h`, with no length prefix.
    pub fn write_hash(&mut self, h: &Hash) -> &mut Self {
        self.0.extend_from_slice(&h.0);
        self
    }

    /// Consumes the encoder, returning the accumulated bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_u8_writes_single_byte() {
        let mut e = Encoder::new();
        e.write_u8(0xab);
        assert_eq!(e.into_bytes(), vec![0xab]);
    }

    #[test]
    fn write_u32_is_little_endian() {
        let mut e = Encoder::new();
        e.write_u32(1);
        assert_eq!(e.into_bytes(), vec![1, 0, 0, 0]);
    }

    #[test]
    fn write_str_is_len_prefixed_no_terminator() {
        let mut e = Encoder::new();
        e.write_str("ab");
        assert_eq!(e.into_bytes(), vec![2, 0, 0, 0, b'a', b'b']);
    }

    #[test]
    fn write_bytes_is_len_prefixed() {
        let mut e = Encoder::new();
        e.write_bytes(&[0xde, 0xad]);
        assert_eq!(e.into_bytes(), vec![2, 0, 0, 0, 0xde, 0xad]);
    }

    #[test]
    fn write_hash_has_no_prefix() {
        let mut e = Encoder::new();
        e.write_hash(&Hash([9u8; 32]));
        assert_eq!(e.into_bytes(), vec![9u8; 32]);
    }

    #[test]
    fn chained_writes_concatenate_in_order() {
        let mut e = Encoder::new();
        e.write_u32(1).write_str("x");
        assert_eq!(e.into_bytes(), vec![1, 0, 0, 0, 1, 0, 0, 0, b'x']);
    }
}
