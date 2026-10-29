//! A revoked device can't sign back in (docs/plan/35-revoke-split.md §1).
//!
//! Revoking a device ends its sessions (D416), and its renewal already said
//! "revoked". Before this, it could still type the account password and get
//! a fresh session that reads (D73 refuses what it signs). Every way into a
//! session now asks the identity first: a device id the identity marks
//! revoked, by a revocation in its manifest (D73's marker row) or by its
//! device record, is refused with [`ErrorCode::RevokedSignIn`] and
//! [`WORDS`]:
//!
//! - the OPAQUE login's finish, after the password and the device's proof
//!   of its key check out (so nobody learns a device's standing without the
//!   password) and before any second factor is asked (no key prompt, no
//!   approval email for a device that can never get in);
//! - the end of every sign-in, [`crate::api::issue_session`]: the second
//!   factor's poll after a security key or the account's email approved it,
//!   so a device revoked while it waited is refused too;
//! - session renewal (`renewal::standing`).
//!
//! The one read a revoked device keeps is `GET /v1/identity` with the
//! token it already holds (`api::check_session`), so it can verify from
//! signed records that it is out and shut its own copy (D73, D416).
//! Coming back means joining again as a new device (D246): a new device id
//! is not revoked.

use axum::http::StatusCode;
use lockdown_sync::wire::ErrorCode;
use lockdown_vault::Id;

use crate::AppState;
use crate::api::{ApiError, ApiResult};

/// What a revoked device is told when it tries to sign in or renew. The
/// client says the same words in its own voice (`SignInReason::Revoked`).
pub const WORDS: &str =
    "This device was revoked from your account. Join it again from a device you still use.";

/// The refusal: 403, `revoked_sign_in`, [`WORDS`].
pub(crate) fn refusal() -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, ErrorCode::RevokedSignIn, WORDS)
}

/// Whether the account's identity marks `device_id` revoked: a revocation
/// in the manifest the server holds, or the device's own record marked
/// revoked in the newest verified record set.
pub(crate) fn is_revoked(state: &AppState, account_id: Id, device_id: Id) -> ApiResult<bool> {
    if state
        .storage
        .revoked_after(account_id, device_id, device_id)?
        .is_some()
    {
        return Ok(true);
    }
    Ok(crate::renewal::listed_device(state, account_id, device_id)?
        .is_some_and(|(_, revoked)| revoked))
}

/// Refuses a sign-in by a device the identity revoked.
pub(crate) fn refuse_revoked(state: &AppState, account_id: Id, device_id: Id) -> ApiResult<()> {
    if is_revoked(state, account_id, device_id)? {
        return Err(refusal());
    }
    Ok(())
}
