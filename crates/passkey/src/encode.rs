//! The two encodings WebAuthn speaks: base64url without padding (every
//! binary value the page sees) and the small part of CBOR the authenticator
//! writes (CTAP2's canonical form: definite lengths, shortest integers, map
//! keys in the order the caller gives, which the callers give canonically).

use data_encoding::BASE64URL_NOPAD;

/// Bytes as base64url without padding.
pub fn b64url(bytes: &[u8]) -> String {
    BASE64URL_NOPAD.encode(bytes)
}

/// base64url text, with or without padding, as bytes.
pub fn from_b64url(text: &str) -> Option<Vec<u8>> {
    BASE64URL_NOPAD
        .decode(text.trim_end_matches('=').as_bytes())
        .ok()
}

/// A CBOR writer for the authenticator's few shapes.
#[derive(Default)]
pub struct Cbor {
    out: Vec<u8>,
}

impl Cbor {
    /// An empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    fn head(&mut self, major: u8, n: u64) {
        let m = major << 5;
        match n {
            0..=23 => self.out.push(m | n as u8),
            24..=0xff => self.out.extend([m | 24, n as u8]),
            0x100..=0xffff => {
                self.out.push(m | 25);
                self.out.extend((n as u16).to_be_bytes());
            }
            0x1_0000..=0xffff_ffff => {
                self.out.push(m | 26);
                self.out.extend((n as u32).to_be_bytes());
            }
            _ => {
                self.out.push(m | 27);
                self.out.extend(n.to_be_bytes());
            }
        }
    }

    /// A signed integer.
    pub fn int(&mut self, v: i64) -> &mut Self {
        if v >= 0 {
            self.head(0, v as u64);
        } else {
            self.head(1, (-1 - v) as u64);
        }
        self
    }

    /// A byte string.
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.head(2, b.len() as u64);
        self.out.extend_from_slice(b);
        self
    }

    /// A text string.
    pub fn text(&mut self, t: &str) -> &mut Self {
        self.head(3, t.len() as u64);
        self.out.extend_from_slice(t.as_bytes());
        self
    }

    /// The head of a map of `n` pairs; the pairs follow.
    pub fn map(&mut self, n: usize) -> &mut Self {
        self.head(5, n as u64);
        self
    }

    /// Bytes already encoded as one CBOR item.
    pub fn raw(&mut self, item: &[u8]) -> &mut Self {
        self.out.extend_from_slice(item);
        self
    }

    /// The encoded bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trips_and_accepts_padding() {
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(from_b64url("-_8").unwrap(), vec![0xfb, 0xff]);
        assert_eq!(from_b64url("-_8=").unwrap(), vec![0xfb, 0xff]);
        assert!(from_b64url("not base64!").is_none());
    }

    #[test]
    fn integers_take_their_shortest_form() {
        let enc = |v: i64| {
            let mut c = Cbor::new();
            c.int(v);
            c.finish()
        };
        assert_eq!(enc(1), [0x01]);
        assert_eq!(enc(-1), [0x20]);
        assert_eq!(enc(-7), [0x26]);
        assert_eq!(enc(24), [0x18, 24]);
        assert_eq!(enc(-257), [0x39, 0x01, 0x00]);
        assert_eq!(enc(70_000), [0x1a, 0, 1, 0x11, 0x70]);
        let mut c = Cbor::new();
        c.map(1).text("fmt").text("none");
        let v: ciborium::Value = ciborium::from_reader(c.finish().as_slice()).unwrap();
        assert_eq!(v, ciborium::Value::Map(vec![("fmt".into(), "none".into())]));
    }
}
