//! Checking someone's keys with a phrase said aloud (`key-transparency`;
//! 04-security §7 "a passphrase-verified invite proves a fingerprint over
//! the network"; docs/plan/22-security-depth.md §9).
//!
//! Comparing thirty digits is the strongest check and the one people skip.
//! A shorter path: the inviter's app shows a nine-digit phrase, the inviter
//! tells it to the other person (in the room, on a call), and the other
//! person types it. The two apps then run OPAQUE (RFC 9807) keyed by the
//! phrase through the server's check relay, the inviter as the "server"
//! with a password file it made from the phrase a moment ago, and confirm
//! the fingerprints each holds under the key the exchange agreed:
//!
//! 1. joiner → inviter: [`CheckFrame::Request`] (the blinded phrase)
//! 2. inviter → joiner: [`CheckFrame::Response`]
//! 3. joiner → inviter: [`CheckFrame::Finish`] (OPAQUE's finalization and
//!    the joiner's tag over its view of both fingerprints)
//! 4. inviter → joiner: [`CheckFrame::Confirm`] (the inviter's tag), or
//!    [`CheckFrame::Failed`]
//!
//! What the server, which relays every frame and hands out the keys being
//! checked, can do about it: OPAQUE gives anyone without the phrase one
//! guess per check (one in a billion), and a check closes after one try
//! either way; it learns nothing it could test offline. When the phrase was
//! right but the two views of the keys differ, the tags fail and both sides
//! say so plainly: the keys the server handed out aren't the other person's.
//! A matched check marks the contact verified, exactly as comparing the
//! digits does.

use ecosystem_identity::cbor::to_canonical;
use lockdown_crypto::auth::{LoginClient, LoginServerState, OpaqueServer, RegistrationClient};
use lockdown_crypto::hash::{derive_key, keyed_hash};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use zeroize::Zeroizing;

use crate::{Error, Id, Result};

/// The frames' version.
pub const CHECK_V: u16 = 1;
/// Digits in a phrase.
pub const PHRASE_DIGITS: usize = 9;

const JOINER_TAG: &str = "lockdown 2026-10-18 phrase check joiner tag";
const INVITER_TAG: &str = "lockdown 2026-10-18 phrase check inviter tag";

/// A nine-digit phrase, shown as three groups of three.
pub struct Phrase(Zeroizing<String>);

impl std::fmt::Debug for Phrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Phrase(…)")
    }
}

impl Phrase {
    /// A fresh phrase from the operating system's randomness, each digit
    /// uniform (bytes of 250 and above are drawn again).
    pub fn generate() -> Result<Self> {
        let mut digits = String::with_capacity(PHRASE_DIGITS);
        while digits.len() < PHRASE_DIGITS {
            let mut b = [0u8; 16];
            lockdown_crypto::rand::random_bytes(&mut b)?;
            for x in b {
                if x < 250 && digits.len() < PHRASE_DIGITS {
                    digits.push(char::from(b'0' + x % 10));
                }
            }
        }
        Ok(Self(Zeroizing::new(digits)))
    }

    /// A phrase as typed: the digits, ignoring spaces and dashes.
    pub fn parse(typed: &str) -> Result<Self> {
        let digits: String = typed
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .collect();
        if digits.len() != PHRASE_DIGITS || !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(Error::Invalid(format!(
                "the phrase is {PHRASE_DIGITS} digits, like 123 456 789"
            )));
        }
        Ok(Self(Zeroizing::new(digits)))
    }

    /// As shown: `123 456 789`.
    pub fn shown(&self) -> String {
        let d = self.0.as_str();
        format!("{} {} {}", &d[..3], &d[3..6], &d[6..])
    }

    fn secret(&self) -> &str {
        self.0.as_str()
    }
}

/// One side's view of what is being checked: each person's account and the
/// fingerprint this side holds for them (its own, computed; the other's,
/// pinned from the server's log).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Views {
    /// The check's id on the relay.
    pub verify_id: Id,
    /// Who opened the check.
    pub inviter: Id,
    /// The inviter's fingerprint, as this side holds it.
    #[serde(with = "serde_bytes")]
    pub inviter_fp: [u8; 32],
    /// Who was asked.
    pub joiner: Id,
    /// The joiner's fingerprint, as this side holds it.
    #[serde(with = "serde_bytes")]
    pub joiner_fp: [u8; 32],
}

impl Views {
    fn tag(&self, context: &str, session_key: &[u8]) -> Result<[u8; 32]> {
        let key = Zeroizing::new(derive_key(context, session_key));
        let transcript = to_canonical(&(CHECK_V, self))?;
        Ok(keyed_hash(&key, &transcript).0)
    }

    fn credential_id(&self) -> Vec<u8> {
        self.verify_id.to_string().into_bytes()
    }
}

/// One frame on the check relay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum CheckFrame {
    /// OPAQUE's credential request.
    Request {
        /// [`CHECK_V`].
        v: u16,
        /// The message.
        message: ByteBuf,
    },
    /// OPAQUE's credential response.
    Response {
        /// [`CHECK_V`].
        v: u16,
        /// The message.
        message: ByteBuf,
    },
    /// OPAQUE's finalization and the joiner's tag.
    Finish {
        /// [`CHECK_V`].
        v: u16,
        /// The finalization.
        finalization: ByteBuf,
        /// The joiner's tag over its view.
        tag: ByteBuf,
    },
    /// The inviter's tag over its view.
    Confirm {
        /// [`CHECK_V`].
        v: u16,
        /// The tag.
        tag: ByteBuf,
    },
    /// The check failed on the other side.
    Failed {
        /// [`CHECK_V`].
        v: u16,
    },
}

impl CheckFrame {
    /// As bytes for the relay.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From the relay's bytes (hostile input).
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let frame: Self = ciborium::from_reader(bytes)
            .map_err(|_| Error::Invalid("that isn't a key-check frame".into()))?;
        let v = match &frame {
            Self::Request { v, .. }
            | Self::Response { v, .. }
            | Self::Finish { v, .. }
            | Self::Confirm { v, .. }
            | Self::Failed { v } => *v,
        };
        if v != CHECK_V {
            return Err(Error::Unsupported(format!("key-check frames v{v}")));
        }
        Ok(frame)
    }
}

/// How a check ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Same phrase, same keys on both sides: the other person is verified.
    Matched,
    /// The phrase didn't match (a typo, or someone else answering).
    WrongPhrase,
    /// The phrase matched but the keys didn't: the keys this side was given
    /// for the other person aren't theirs. Don't share with them.
    KeysDiffer,
}

/// What to do after a frame.
#[derive(Debug, Default)]
pub struct Step {
    /// A frame to send, if any.
    pub send: Option<Vec<u8>>,
    /// How the check ended, once it has.
    pub outcome: Option<Outcome>,
}

fn failed() -> Result<Vec<u8>> {
    CheckFrame::Failed { v: CHECK_V }.to_bytes()
}

fn same(a: &[u8], b: &[u8; 32]) -> bool {
    a.len() == 32 && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The inviter's side.
pub struct Inviter {
    views: Views,
    server: OpaqueServer,
    file: Vec<u8>,
    state: Option<LoginServerState>,
    done: bool,
}

impl std::fmt::Debug for Inviter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Inviter(…)")
    }
}

impl Inviter {
    /// Makes this check's password file from the phrase, under a server
    /// key made for this check alone.
    pub fn new(phrase: &Phrase, views: Views) -> Result<Self> {
        let server = OpaqueServer::generate()?;
        let (client, request) = RegistrationClient::start(phrase.secret())?;
        let response = server.registration_response(&views.credential_id(), &request)?;
        let (upload, _export) = client.finish(phrase.secret(), &response)?;
        let file = OpaqueServer::password_file(&upload)?;
        Ok(Self {
            views,
            server,
            file,
            state: None,
            done: false,
        })
    }

    /// Takes the joiner's frame.
    pub fn on_frame(&mut self, bytes: &[u8]) -> Result<Step> {
        if self.done {
            return Err(Error::Invalid("this check is over".into()));
        }
        match CheckFrame::parse(bytes)? {
            CheckFrame::Request { message, .. } if self.state.is_none() => {
                let (response, state) = self.server.login_response(
                    &self.views.credential_id(),
                    Some(&self.file),
                    &message,
                )?;
                self.state = Some(state);
                Ok(Step {
                    send: Some(
                        CheckFrame::Response {
                            v: CHECK_V,
                            message: ByteBuf::from(response),
                        }
                        .to_bytes()?,
                    ),
                    outcome: None,
                })
            }
            CheckFrame::Finish {
                finalization, tag, ..
            } => {
                let state = self
                    .state
                    .take()
                    .ok_or_else(|| Error::Invalid("a finish before any request".into()))?;
                self.done = true;
                let Ok(key) = OpaqueServer::login_finish(&state, &finalization) else {
                    return Ok(Step {
                        send: Some(failed()?),
                        outcome: Some(Outcome::WrongPhrase),
                    });
                };
                let want = self.views.tag(JOINER_TAG, key.expose())?;
                if !same(&tag, &want) {
                    return Ok(Step {
                        send: Some(failed()?),
                        outcome: Some(Outcome::KeysDiffer),
                    });
                }
                let mine = self.views.tag(INVITER_TAG, key.expose())?;
                Ok(Step {
                    send: Some(
                        CheckFrame::Confirm {
                            v: CHECK_V,
                            tag: ByteBuf::from(mine.to_vec()),
                        }
                        .to_bytes()?,
                    ),
                    outcome: Some(Outcome::Matched),
                })
            }
            CheckFrame::Failed { .. } => {
                self.done = true;
                Ok(Step {
                    send: None,
                    outcome: Some(Outcome::WrongPhrase),
                })
            }
            _ => Err(Error::Invalid("a key-check frame out of turn".into())),
        }
    }
}

/// The joiner's side.
pub struct Joiner {
    views: Views,
    phrase: Phrase,
    client: Option<LoginClient>,
    session_key: Option<Zeroizing<Vec<u8>>>,
    done: bool,
}

impl std::fmt::Debug for Joiner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Joiner(…)")
    }
}

impl Joiner {
    /// Starts with the phrase the inviter said; returns the first frame.
    pub fn start(phrase: Phrase, views: Views) -> Result<(Self, Vec<u8>)> {
        let (client, request) = LoginClient::start(phrase.secret())?;
        let frame = CheckFrame::Request {
            v: CHECK_V,
            message: ByteBuf::from(request),
        }
        .to_bytes()?;
        Ok((
            Self {
                views,
                phrase,
                client: Some(client),
                session_key: None,
                done: false,
            },
            frame,
        ))
    }

    /// Takes the inviter's frame.
    pub fn on_frame(&mut self, bytes: &[u8]) -> Result<Step> {
        if self.done {
            return Err(Error::Invalid("this check is over".into()));
        }
        match CheckFrame::parse(bytes)? {
            CheckFrame::Response { message, .. } if self.client.is_some() => {
                let client = self.client.take().expect("checked above");
                let Ok(finished) = client.finish(self.phrase.secret(), &message) else {
                    self.done = true;
                    return Ok(Step {
                        send: Some(failed()?),
                        outcome: Some(Outcome::WrongPhrase),
                    });
                };
                let key = Zeroizing::new(finished.session_key.expose().to_vec());
                let tag = self.views.tag(JOINER_TAG, &key)?;
                self.session_key = Some(key);
                Ok(Step {
                    send: Some(
                        CheckFrame::Finish {
                            v: CHECK_V,
                            finalization: ByteBuf::from(finished.finalization),
                            tag: ByteBuf::from(tag.to_vec()),
                        }
                        .to_bytes()?,
                    ),
                    outcome: None,
                })
            }
            CheckFrame::Confirm { tag, .. } => {
                let key = self
                    .session_key
                    .take()
                    .ok_or_else(|| Error::Invalid("a confirmation before the exchange".into()))?;
                self.done = true;
                let want = self.views.tag(INVITER_TAG, &key)?;
                Ok(Step {
                    send: None,
                    outcome: Some(if same(&tag, &want) {
                        Outcome::Matched
                    } else {
                        Outcome::KeysDiffer
                    }),
                })
            }
            CheckFrame::Failed { .. } => {
                self.done = true;
                // After a good exchange, a failure means the inviter's view
                // of the keys differed from this one.
                Ok(Step {
                    send: None,
                    outcome: Some(if self.session_key.is_some() {
                        Outcome::KeysDiffer
                    } else {
                        Outcome::WrongPhrase
                    }),
                })
            }
            _ => Err(Error::Invalid("a key-check frame out of turn".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> &'static (Id, Id, Id) {
        static IDS: std::sync::OnceLock<(Id, Id, Id)> = std::sync::OnceLock::new();
        IDS.get_or_init(|| (Id::new(), Id::new(), Id::new()))
    }

    fn views(inviter_fp: u8, joiner_fp: u8) -> Views {
        let (verify_id, inviter, joiner) = *ids();
        Views {
            verify_id,
            inviter,
            inviter_fp: [inviter_fp; 32],
            joiner,
            joiner_fp: [joiner_fp; 32],
        }
    }

    fn run(said: &str, typed: &str, inviter_view: Views, joiner_view: Views) -> (Outcome, Outcome) {
        let mut inviter = Inviter::new(&Phrase::parse(said).unwrap(), inviter_view).unwrap();
        let (mut joiner, request) =
            Joiner::start(Phrase::parse(typed).unwrap(), joiner_view).unwrap();
        let response = inviter.on_frame(&request).unwrap().send.unwrap();
        let step = joiner.on_frame(&response).unwrap();
        let finish = step.send.unwrap();
        if let Some(out) = step.outcome {
            let inv = inviter.on_frame(&finish).unwrap().outcome.unwrap();
            return (inv, out);
        }
        let back = inviter.on_frame(&finish).unwrap();
        let joined = joiner
            .on_frame(&back.send.unwrap())
            .unwrap()
            .outcome
            .unwrap();
        (back.outcome.unwrap(), joined)
    }

    #[test]
    fn the_same_phrase_and_the_same_keys_verify_both_ways() {
        assert_eq!(
            run("123 456 789", "123-456-789", views(1, 2), views(1, 2)),
            (Outcome::Matched, Outcome::Matched)
        );
    }

    #[test]
    fn a_wrong_phrase_fails_on_both_sides() {
        assert_eq!(
            run("123 456 789", "123 456 788", views(1, 2), views(1, 2)),
            (Outcome::WrongPhrase, Outcome::WrongPhrase)
        );
    }

    #[test]
    fn a_swapped_key_is_caught_even_with_the_right_phrase() {
        // The joiner was handed another key for the inviter.
        assert_eq!(
            run("123 456 789", "123 456 789", views(1, 2), views(9, 2)),
            (Outcome::KeysDiffer, Outcome::KeysDiffer)
        );
        // The inviter was handed another key for the joiner.
        assert_eq!(
            run("123 456 789", "123 456 789", views(1, 9), views(1, 2)),
            (Outcome::KeysDiffer, Outcome::KeysDiffer)
        );
    }

    #[test]
    fn phrases_are_nine_digits_and_frames_are_checked() {
        let p = Phrase::generate().unwrap();
        assert_eq!(p.shown().len(), 11);
        assert!(Phrase::parse(&p.shown()).is_ok());
        assert!(Phrase::parse("12345678").is_err());
        assert!(Phrase::parse("12345678a").is_err());
        assert!(CheckFrame::parse(b"junk").is_err());
        let wrong_v = to_canonical(&CheckFrame::Failed { v: 9 }).unwrap();
        assert!(CheckFrame::parse(&wrong_v).is_err());
        // A frame out of turn is refused, not answered.
        let mut inviter = Inviter::new(&p, views(1, 2)).unwrap();
        let confirm = CheckFrame::Confirm {
            v: CHECK_V,
            tag: ByteBuf::from(vec![0; 32]),
        }
        .to_bytes()
        .unwrap();
        assert!(inviter.on_frame(&confirm).is_err());
    }
}
