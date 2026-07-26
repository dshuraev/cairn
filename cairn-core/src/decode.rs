//! Canonical decoding primitives (§4.1), the inverse of [`crate::encode::Encoder`].
//!
//! Every `decode_canonical` on a §3 object is built from these primitives, so
//! decoding is exactly as strict as encoding was permissive: any truncated or
//! trailing-byte input is rejected rather than silently accepted.

use crate::hash::Hash;
use std::fmt;

/// An error encountered while decoding a canonical byte encoding.
#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The input ended before the expected field could be read.
    UnexpectedEof,
    /// A length-prefixed string's bytes were not valid UTF-8.
    InvalidUtf8,
    /// A `kind_tag` (or similar enum discriminant) had no known meaning.
    InvalidTag(u8),
    /// Bytes remained after decoding was expected to consume the whole input.
    TrailingBytes,
    /// A DirTreeBundle version byte was not structurally parseable by this decoder.
    UnsupportedBundleVersion(u8),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::UnexpectedEof => write!(f, "unexpected end of input"),
            DecodeError::InvalidUtf8 => write!(f, "invalid UTF-8 in encoded string"),
            DecodeError::InvalidTag(tag) => write!(f, "invalid tag byte: {tag}"),
            DecodeError::TrailingBytes => write!(f, "trailing bytes after decoded object"),
            DecodeError::UnsupportedBundleVersion(v) => {
                write!(f, "unsupported bundle version: {v}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

/// Reads bytes for an object's canonical encoding, in lockstep with the
/// [`crate::encode::Encoder`] calls that produced them.
pub struct Decoder<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    /// Creates a decoder over `bytes`.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::UnexpectedEof)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(DecodeError::UnexpectedEof)?;
        self.pos = end;
        Ok(slice)
    }

    /// Reads a single byte.
    pub fn read_u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    /// Reads a little-endian `u32`.
    pub fn read_u32(&mut self) -> Result<u32, DecodeError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| DecodeError::UnexpectedEof)?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Reads a `u32` length prefix followed by that many bytes, interpreted as UTF-8.
    pub fn read_str(&mut self) -> Result<String, DecodeError> {
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)
    }

    /// Reads a `u32` length prefix followed by that many raw bytes.
    pub fn read_bytes(&mut self) -> Result<Vec<u8>, DecodeError> {
        let len = self.read_u32()? as usize;
        Ok(self.take(len)?.to_vec())
    }

    /// Reads a raw 32-byte hash, with no length prefix.
    pub fn read_hash(&mut self) -> Result<Hash, DecodeError> {
        let bytes: [u8; 32] = self
            .take(32)?
            .try_into()
            .map_err(|_| DecodeError::UnexpectedEof)?;
        Ok(Hash(bytes))
    }

    /// Whether every byte has been consumed. Callers that decode a whole
    /// top-level object should check this to reject trailing garbage.
    pub fn is_empty(&self) -> bool {
        self.pos == self.bytes.len()
    }

    /// Errors with [`DecodeError::TrailingBytes`] if bytes remain unconsumed.
    pub fn finish(self) -> Result<(), DecodeError> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::encode::Encoder;

    #[test]
    fn round_trips_all_primitives() {
        let mut e = Encoder::new();
        e.write_u8(7);
        e.write_u32(1234);
        e.write_str("hello");
        e.write_bytes(&[1, 2, 3]);
        e.write_hash(&Hash([9u8; 32]));
        let bytes = e.into_bytes();

        let mut d = Decoder::new(&bytes);
        assert_eq!(d.read_u8().unwrap(), 7);
        assert_eq!(d.read_u32().unwrap(), 1234);
        assert_eq!(d.read_str().unwrap(), "hello");
        assert_eq!(d.read_bytes().unwrap(), vec![1, 2, 3]);
        assert_eq!(d.read_hash().unwrap(), Hash([9u8; 32]));
        d.finish().unwrap();
    }

    #[test]
    fn truncated_input_is_unexpected_eof() {
        let mut d = Decoder::new(&[1, 0, 0]);
        assert_eq!(d.read_u32(), Err(DecodeError::UnexpectedEof));
    }

    #[test]
    fn trailing_bytes_after_finish_is_rejected() {
        let mut d = Decoder::new(&[1, 2, 3]);
        d.read_u8().unwrap();
        assert_eq!(d.finish(), Err(DecodeError::TrailingBytes));
    }

    #[test]
    fn invalid_utf8_str_is_rejected() {
        let mut e = Encoder::new();
        e.write_bytes(&[0xff, 0xfe]);
        let bytes = e.into_bytes();
        let mut d = Decoder::new(&bytes);
        assert_eq!(d.read_str(), Err(DecodeError::InvalidUtf8));
    }
}
