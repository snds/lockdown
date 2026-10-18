//! `clientDataJSON` (WebAuthn L3 §5.8.1), written the way §5.8.1.1 says a
//! client serialises it, so a relying party's "limited verification"
//! parser reads it too. Lockdown is the client here: the origin comes from
//! the browser's record of the tab, through the extension, and is never
//! taken from the page.

use crate::encode::b64url;

/// Which ceremony.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ceremony {
    /// `navigator.credentials.create`.
    Create,
    /// `navigator.credentials.get`.
    Get,
}

impl Ceremony {
    /// The `type` member.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "webauthn.create",
            Self::Get => "webauthn.get",
        }
    }
}

/// The collected client data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientData {
    pub ceremony: Ceremony,
    /// The RP's challenge, as bytes.
    pub challenge: Vec<u8>,
    /// The caller's serialised origin.
    pub origin: String,
    /// The top-level origin when the caller is a cross-origin frame
    /// (`crossOrigin` is then `true`).
    pub top_origin: Option<String>,
}

/// CCDToString (§5.8.1.1): a JSON string where only `"`, `\` and the
/// characters below U+0020 are escaped, the last as `\u` and four
/// lowercase hex digits.
fn ccd_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

impl ClientData {
    /// The JSON text, members in the spec's order: `type`, `challenge`,
    /// `origin`, `crossOrigin`, then `topOrigin` when cross-origin.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\"type\":");
        ccd_string(&mut out, self.ceremony.as_str());
        out.push_str(",\"challenge\":");
        ccd_string(&mut out, &b64url(&self.challenge));
        out.push_str(",\"origin\":");
        ccd_string(&mut out, &self.origin);
        match &self.top_origin {
            Some(top) => {
                out.push_str(",\"crossOrigin\":true,\"topOrigin\":");
                ccd_string(&mut out, top);
            }
            None => out.push_str(",\"crossOrigin\":false"),
        }
        out.push('}');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spec_order_and_escaping() {
        let cd = ClientData {
            ceremony: Ceremony::Get,
            challenge: vec![0xfb, 0xff, 0x00],
            origin: "https://example.com".into(),
            top_origin: None,
        };
        assert_eq!(
            cd.to_json(),
            r#"{"type":"webauthn.get","challenge":"-_8A","origin":"https://example.com","crossOrigin":false}"#
        );
        let framed = ClientData {
            top_origin: Some("https://news.test".into()),
            ..cd.clone()
        };
        assert!(
            framed
                .to_json()
                .ends_with(r#""crossOrigin":true,"topOrigin":"https://news.test"}"#)
        );
        let mut s = String::new();
        ccd_string(&mut s, "a\"b\\c\u{1}é");
        assert_eq!(s, "\"a\\\"b\\\\c\\u0001é\"");
        // Any JSON parser reads it.
        let v: serde_json::Value = serde_json::from_str(&cd.to_json()).unwrap();
        assert_eq!(v["origin"], "https://example.com");
    }
}
