//! Session renewal (D52 as changed by D375; docs/plan/33-session-renewal.md):
//! a signed-in device renews its own session with its device key, so the
//! account password is asked only on a new device, after the session was
//! ended, after the device was revoked, or after a long time unused.
//!
//! Two round trips, both carrying the old token as the bearer (it may have
//! run out; the server checks its signature, not its age):
//!
//! 1. `POST /v1/account/session/challenge` → [`RenewChallenge`]: a one-time
//!    id and 32 random bytes, bound on the server to the session the token
//!    names, for two minutes.
//! 2. The device signs [`session_renew_message`] (the label
//!    `lockdown/session-renew/v1`, the challenge, and the BLAKE3 hash of
//!    the old token) with its device key: the key the identity's records
//!    list and the server holds for the account.
//! 3. `POST /v1/account/session/renew` with [`RenewFinish`] →
//!    [`LoginFinishReply`]: a new token and session id; the old session
//!    is ended in the same step.
//!
//! The token alone renews nothing (the signature needs the device key),
//! and the signature alone renews nothing (it names the token's hash and
//! a challenge given for that token's session). Everything the server
//! answers is checked here as hostile: the reply must be a plain token of
//! a sane size and must not ask for a second factor.
//!
//! When the server says no, [`SignInReason`] says why in words a person
//! understands, so the password prompt can say which of the cases it is.

use lockdown_crypto::auth::session_renew_message;

use crate::transport::{HttpTransport, Method, TransportError};
use crate::wire::{ErrorCode, LoginFinishReply, RENEW_V, RenewChallenge, RenewFinish};

/// Renew this long before the token runs out (seconds), so a busy device
/// never meets `token_expired` at all; one that slept renews on its next
/// call.
pub const RENEW_MARGIN_SECS: i64 = 5 * 60;

/// The longest token this client accepts from a renewal.
const TOKEN_MAX: usize = 4096;

/// Why only the account password helps now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignInReason {
    /// The session was ended from one of the account's devices or by the
    /// server's operator.
    Ended,
    /// This device was revoked from the identity.
    Revoked,
    /// The session went unused for longer than the server's idle limit.
    Idle,
    /// The server doesn't renew sessions (an older server, or renewal
    /// turned off), and the token ran out.
    NotRenewable,
    /// The server doesn't recognise the session or this device's key any
    /// more (its keys were reset, or the session is from another server).
    Unknown,
}

impl SignInReason {
    /// Every reason, for classifying words back into one.
    pub const ALL: [SignInReason; 5] = [
        SignInReason::Ended,
        SignInReason::Revoked,
        SignInReason::Idle,
        SignInReason::NotRenewable,
        SignInReason::Unknown,
    ];

    /// The machine word a shell switches on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ended => "ended",
            Self::Revoked => "revoked",
            Self::Idle => "idle",
            Self::NotRenewable => "not_renewable",
            Self::Unknown => "unknown",
        }
    }

    /// The reason a machine word names.
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == word)
    }

    /// What the person reads beside the password prompt.
    pub fn message(self) -> &'static str {
        match self {
            Self::Ended => {
                "This device's session was ended from one of your devices or by whoever runs the server, so it can't renew itself. Log in again with your account password."
            }
            Self::Revoked => {
                "This device was revoked from your account on another device, so its session can't be renewed. Log in again with your account password."
            }
            Self::Idle => {
                "This device went unused for longer than the server allows (30 days unless whoever runs it changed that), so its session can't be renewed. Log in again with your account password."
            }
            Self::NotRenewable => {
                "This server doesn't let a device renew its own session, and the session has run out. Log in again with your account password."
            }
            Self::Unknown => {
                "The server no longer recognises this device's session, so it can't renew itself. Log in again with your account password."
            }
        }
    }

    /// The reason whose words `text` carries, if any (an error that went
    /// through a `String` on its way to a shell).
    pub fn in_text(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| text.contains(r.message()))
    }
}

/// Why a renewal didn't happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenewError {
    /// The server refused for a reason only the password fixes.
    SignIn(SignInReason),
    /// Not this time (no network, the server busy or answering nonsense);
    /// the next call tries again.
    Failed(String),
}

impl std::fmt::Display for RenewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SignIn(r) => f.write_str(r.message()),
            Self::Failed(why) => write!(f, "couldn't renew the session with the server: {why}"),
        }
    }
}

impl std::error::Error for RenewError {}

/// A renewed session.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Renewed {
    /// The new bearer token.
    pub token: String,
    /// When it runs out (seconds since the Unix epoch).
    pub expires_at: i64,
    /// The profile the account is for, as the server says.
    pub profile_id: lockdown_vault::Id,
}

/// The two round trips, over whatever carries them: [`HttpTransport`]
/// here, a fake in the tests.
pub trait RenewRounds {
    /// `POST /v1/account/session/challenge` with `token` as the bearer.
    fn renew_challenge(&self, token: &str) -> crate::transport::Result<RenewChallenge>;
    /// `POST /v1/account/session/renew` with `token` as the bearer.
    fn renew_finish(
        &self,
        token: &str,
        finish: &RenewFinish,
    ) -> crate::transport::Result<LoginFinishReply>;
}

impl RenewRounds for HttpTransport {
    fn renew_challenge(&self, token: &str) -> crate::transport::Result<RenewChallenge> {
        self.call::<(), _>(
            Method::Post,
            "/v1/account/session/challenge",
            Some(token),
            None,
        )
    }

    fn renew_finish(
        &self,
        token: &str,
        finish: &RenewFinish,
    ) -> crate::transport::Result<LoginFinishReply> {
        self.call(
            Method::Post,
            "/v1/account/session/renew",
            Some(token),
            Some(finish),
        )
    }
}

/// What a refusal from either round trip means for this device.
fn refusal(e: TransportError) -> RenewError {
    match e {
        TransportError::Refused { code, status, .. } => match code {
            ErrorCode::SessionEnded => RenewError::SignIn(SignInReason::Ended),
            ErrorCode::RevokedDevice => RenewError::SignIn(SignInReason::Revoked),
            ErrorCode::SessionIdle => RenewError::SignIn(SignInReason::Idle),
            ErrorCode::RenewalOff => RenewError::SignIn(SignInReason::NotRenewable),
            ErrorCode::Unauthenticated | ErrorCode::BadSignature | ErrorCode::UnknownDevice => {
                RenewError::SignIn(SignInReason::Unknown)
            }
            // A server from before renewal has no such route.
            _ if status == 404 || status == 405 => RenewError::SignIn(SignInReason::NotRenewable),
            _ => RenewError::Failed(format!("the server refused (HTTP {status})")),
        },
        TransportError::Network(why) => RenewError::Failed(why),
        TransportError::Decode(why) => {
            RenewError::Failed(format!("the answer didn't decode: {why}"))
        }
    }
}

/// Renews `token`'s session: asks for a challenge, signs it with the
/// device key through `sign`, and takes the new session. The old token
/// stops working once this returns a new one.
pub fn renew(
    rounds: &dyn RenewRounds,
    token: &str,
    sign: &dyn Fn(&[u8]) -> lockdown_crypto::Result<[u8; 64]>,
) -> Result<Renewed, RenewError> {
    if token.is_empty() {
        return Err(RenewError::SignIn(SignInReason::Ended));
    }
    let challenge = rounds.renew_challenge(token).map_err(refusal)?;
    if challenge.v != RENEW_V {
        return Err(RenewError::Failed(format!(
            "the server's renewal challenge is format {}, not {RENEW_V}",
            challenge.v
        )));
    }
    let message = session_renew_message(&challenge.challenge_id, &challenge.challenge, token);
    let signature = sign(&message).map_err(|e| RenewError::Failed(e.to_string()))?;
    let reply = rounds
        .renew_finish(
            token,
            &RenewFinish {
                v: RENEW_V,
                challenge_id: challenge.challenge_id,
                signature: signature.to_vec(),
            },
        )
        .map_err(refusal)?;
    if reply.second_factor.is_some() {
        return Err(RenewError::Failed(
            "the server asked for a second factor to renew a session, which renewal never needs"
                .into(),
        ));
    }
    let sane = !reply.token.is_empty()
        && reply.token.len() <= TOKEN_MAX
        && reply.token.bytes().all(|b| b.is_ascii_graphic())
        && reply.token != token;
    if !sane {
        return Err(RenewError::Failed(
            "the server's renewed token isn't one this device can use".into(),
        ));
    }
    Ok(Renewed {
        token: reply.token,
        expires_at: reply.expires_at,
        profile_id: reply.profile_id,
    })
}

/// Whether a token running out at `expires_at` should be renewed at `now`.
pub fn renewal_due(expires_at: i64, now: i64) -> bool {
    expires_at.saturating_sub(now) <= RENEW_MARGIN_SECS
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use lockdown_crypto::SigningKey;
    use lockdown_vault::Id;

    use super::*;

    /// A server in a box: hands out one challenge and checks the signature
    /// over exactly the message the protocol names.
    struct Fake {
        key: lockdown_crypto::VerifyingKey,
        challenge: RenewChallenge,
        refuse_start: Option<(u16, ErrorCode)>,
        refuse_finish: Option<(u16, ErrorCode)>,
        reply: RefCell<Option<LoginFinishReply>>,
        seen: RefCell<Vec<Vec<u8>>>,
    }

    fn refused(status: u16, code: ErrorCode) -> TransportError {
        TransportError::Refused {
            status,
            code,
            message: "no".into(),
            head: None,
        }
    }

    impl Fake {
        fn new(key: &SigningKey) -> Self {
            Self {
                key: key.verifying_key(),
                challenge: RenewChallenge {
                    v: RENEW_V,
                    challenge_id: Id::new(),
                    challenge: [7; 32],
                    expires_at: 100,
                },
                refuse_start: None,
                refuse_finish: None,
                reply: RefCell::new(Some(LoginFinishReply {
                    token: "lk1.new.token".into(),
                    expires_at: 2_000,
                    profile_id: Id::new(),
                    second_factor: None,
                })),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl RenewRounds for Fake {
        fn renew_challenge(&self, _token: &str) -> crate::transport::Result<RenewChallenge> {
            if let Some((s, c)) = self.refuse_start {
                return Err(refused(s, c));
            }
            Ok(self.challenge.clone())
        }

        fn renew_finish(
            &self,
            token: &str,
            finish: &RenewFinish,
        ) -> crate::transport::Result<LoginFinishReply> {
            if let Some((s, c)) = self.refuse_finish {
                return Err(refused(s, c));
            }
            assert_eq!(finish.challenge_id, self.challenge.challenge_id);
            let message = session_renew_message(
                &self.challenge.challenge_id,
                &self.challenge.challenge,
                token,
            );
            self.seen.borrow_mut().push(message.clone());
            self.key
                .verify(&message, &finish.signature, "renewal")
                .map_err(|_| refused(401, ErrorCode::BadSignature))?;
            Ok(self.reply.borrow_mut().take().expect("one renewal"))
        }
    }

    fn signer(seed: [u8; 32]) -> impl Fn(&[u8]) -> lockdown_crypto::Result<[u8; 64]> {
        let key = SigningKey::from_bytes(seed);
        move |m| Ok(key.sign(m))
    }

    fn seed() -> [u8; 32] {
        SigningKey::generate().unwrap().inner().to_bytes()
    }

    #[test]
    fn the_device_signs_the_challenge_under_its_own_context_and_takes_the_new_token() {
        let key = seed();
        let fake = Fake::new(&SigningKey::from_bytes(key));
        let renewed = renew(&fake, "lk1.old.token", &signer(key)).unwrap();
        assert_eq!(renewed.token, "lk1.new.token");
        assert_eq!(renewed.expires_at, 2_000);
        let seen = fake.seen.borrow();
        assert!(seen[0].starts_with(b"lockdown/session-renew/v1\0"));
        // The token itself is not in the message, only its hash.
        assert!(!seen[0].windows(13).any(|w| w == b"lk1.old.token"));
    }

    #[test]
    fn another_devices_key_is_refused_and_reads_as_unknown() {
        let fake = Fake::new(&SigningKey::generate().unwrap());
        assert_eq!(
            renew(&fake, "lk1.old.token", &signer(seed())),
            Err(RenewError::SignIn(SignInReason::Unknown))
        );
    }

    #[test]
    fn each_refusal_names_its_reason_and_transient_ones_ask_for_nothing() {
        let seed = seed();
        let key = SigningKey::from_bytes(seed);
        for (status, code, want) in [
            (
                401,
                ErrorCode::SessionEnded,
                RenewError::SignIn(SignInReason::Ended),
            ),
            (
                403,
                ErrorCode::RevokedDevice,
                RenewError::SignIn(SignInReason::Revoked),
            ),
            (
                401,
                ErrorCode::SessionIdle,
                RenewError::SignIn(SignInReason::Idle),
            ),
            (
                403,
                ErrorCode::RenewalOff,
                RenewError::SignIn(SignInReason::NotRenewable),
            ),
            (
                404,
                ErrorCode::Internal,
                RenewError::SignIn(SignInReason::NotRenewable),
            ),
            (
                401,
                ErrorCode::Unauthenticated,
                RenewError::SignIn(SignInReason::Unknown),
            ),
            (
                429,
                ErrorCode::RateLimited,
                RenewError::Failed("the server refused (HTTP 429)".into()),
            ),
            (
                400,
                ErrorCode::BadRequest,
                RenewError::Failed("the server refused (HTTP 400)".into()),
            ),
        ] {
            let mut at_start = Fake::new(&key);
            at_start.refuse_start = Some((status, code));
            assert_eq!(
                renew(&at_start, "lk1.t", &signer(seed)),
                Err(want.clone()),
                "{code:?} at the challenge"
            );
            let mut at_finish = Fake::new(&key);
            at_finish.refuse_finish = Some((status, code));
            assert_eq!(
                renew(&at_finish, "lk1.t", &signer(seed)),
                Err(want),
                "{code:?} at the renewal"
            );
        }
        // No token, nothing to renew: the session is gone.
        assert_eq!(
            renew(&Fake::new(&key), "", &signer(seed)),
            Err(RenewError::SignIn(SignInReason::Ended))
        );
    }

    #[test]
    fn a_hostile_reply_is_not_taken() {
        let seed = seed();
        let key = SigningKey::from_bytes(seed);
        let bad = [
            ("", None),
            ("lk1.old.token", None),
            ("has space", None),
            ("ok", Some(())),
        ];
        for (token, second) in bad {
            let fake = Fake::new(&key);
            *fake.reply.borrow_mut() = Some(LoginFinishReply {
                token: token.into(),
                expires_at: 1,
                profile_id: Id::new(),
                second_factor: second.map(|()| {
                    let pending_id = Id::new();
                    crate::account_security::SecondFactorPending {
                        pending_id,
                        poll_key: serde_bytes::ByteBuf::from(vec![1; 32]),
                        request: crate::account_security::SecondFactorRequest {
                            pending_id,
                            challenge: serde_bytes::ByteBuf::from(vec![2; 32]),
                            rp_id: "vault.example".into(),
                            allow: Vec::new(),
                            timeout_ms: 1,
                            expires_at: 1,
                            device_label: "x".into(),
                            email_to: None,
                        },
                    }
                }),
            });
            assert!(
                matches!(
                    renew(&fake, "lk1.old.token", &signer(seed)),
                    Err(RenewError::Failed(_))
                ),
                "{token:?}"
            );
        }
        let mut newer = Fake::new(&key);
        newer.challenge.v = 2;
        assert!(matches!(
            renew(&newer, "lk1.old.token", &signer(seed)),
            Err(RenewError::Failed(_))
        ));
    }

    #[test]
    fn reasons_round_trip_through_words_and_machine_names() {
        for r in SignInReason::ALL {
            assert_eq!(SignInReason::parse(r.as_str()), Some(r));
            let wrapped = format!("sync stopped: {}", r.message());
            assert_eq!(SignInReason::in_text(&wrapped), Some(r));
        }
        assert_eq!(SignInReason::in_text("the server refused: no"), None);
        assert!(renewal_due(1_000, 1_000 - RENEW_MARGIN_SECS));
        assert!(!renewal_due(1_000, 1_000 - RENEW_MARGIN_SECS - 1));
        assert!(renewal_due(1_000, 5_000));
    }
}
