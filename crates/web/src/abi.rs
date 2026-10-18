//! The module's boundary with the worker (WASI builds only).
//!
//! Exports, all `extern "C"`:
//!
//! - `ld_alloc(len) -> ptr` and `ld_free(ptr, len)`: buffers the worker
//!   writes a request into and reads an answer out of. `ld_free` zeroes
//!   the buffer before it goes back, because an answer may be a secret.
//! - `ld_init()`: makes the vault (call once, after `_initialize`).
//! - `ld_call(ptr, len) -> u64`: one command. The request is the JSON
//!   `{"cmd": "...", "args": {...}}`. The answer is a buffer whose first
//!   byte says what follows: `0` JSON, `1` raw bytes, `2` an error (JSON
//!   `{code, message, detail}`, the app's own). The return value packs the
//!   answer's pointer in the high 32 bits and its length in the low 32.
//!
//! Imports, from the module `lockdown` (the worker provides them):
//!
//! - `http_send(ptr, len) -> u32`: one HTTP request. The request is a
//!   little-endian `u32` length, that many bytes of JSON
//!   `{method, url, contentType, authorization}`, then the body. The
//!   worker answers synchronously and returns the length of the answer it
//!   holds: a little-endian `u16` status (`0` for no answer), then the
//!   body (or, with status `0`, why there was none).
//! - `http_take(ptr)`: copies that held answer to `ptr` and forgets it.
//!
//! The worker refuses any URL outside the page's own origin, so a hostile
//! answer cannot steer a request anywhere else.

use std::cell::RefCell;

use lockdown_sync::{HttpRequest, install_http_hook};
use zeroize::Zeroize;

use crate::commands::{Reply, call};
use crate::host::{WebPaths, WebVault};

#[link(wasm_import_module = "lockdown")]
unsafe extern "C" {
    fn http_send(ptr: *const u8, len: usize) -> u32;
    fn http_take(ptr: *mut u8);
}

thread_local! {
    static VAULT: RefCell<Option<WebVault>> = const { RefCell::new(None) };
}

/// A buffer of `len` bytes for the worker to fill.
#[unsafe(no_mangle)]
pub extern "C" fn ld_alloc(len: usize) -> *mut u8 {
    let buf = vec![0u8; len].into_boxed_slice();
    Box::into_raw(buf).cast::<u8>()
}

/// Zeroes and frees a buffer from [`ld_alloc`] or an answer of [`ld_call`].
///
/// # Safety
///
/// `ptr` and `len` must be exactly one buffer this module handed out, not
/// freed before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ld_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the caller hands back a buffer this module made as a boxed
    // slice of exactly `len` bytes.
    let mut buf = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) };
    buf.zeroize();
}

/// Makes the vault and installs the HTTP hook. Calling it again is harmless.
#[unsafe(no_mangle)]
pub extern "C" fn ld_init() {
    install_http_hook(send);
    VAULT.with(|v| {
        let mut v = v.borrow_mut();
        if v.is_none() {
            let vault = WebVault::new(WebPaths::in_worker());
            // The worker passes the page's origin in the WASI environment.
            *v = Some(match std::env::var("LOCKDOWN_ORIGIN") {
                Ok(origin) if !origin.is_empty() => vault.serving(&origin),
                _ => vault,
            });
        }
    });
}

fn answer(tag: u8, payload: &[u8]) -> u64 {
    let mut out = vec![0u8; payload.len() + 1].into_boxed_slice();
    out[0] = tag;
    out[1..].copy_from_slice(payload);
    let len = out.len();
    let ptr = Box::into_raw(out).cast::<u8>();
    ((ptr as usize as u64) << 32) | len as u64
}

fn error_answer(e: &lockdown_app::AppError) -> u64 {
    let body = serde_json::to_vec(e).unwrap_or_else(|_| b"{}".to_vec());
    answer(2, &body)
}

/// One command: see the module doc for the request and the answer.
///
/// # Safety
///
/// `ptr` and `len` must name `len` readable bytes (a buffer from
/// [`ld_alloc`]); the caller still frees it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ld_call(ptr: *const u8, len: usize) -> u64 {
    // SAFETY: the worker wrote `len` bytes at `ptr` in a buffer from `ld_alloc`.
    let request = unsafe { std::slice::from_raw_parts(ptr, len) };
    #[derive(serde::Deserialize)]
    struct Request {
        cmd: String,
        #[serde(default)]
        args: serde_json::Value,
    }
    let Ok(Request { cmd, args }) = serde_json::from_slice::<Request>(request) else {
        return error_answer(&lockdown_app::AppError::refused(
            "the request didn't decode",
        ));
    };
    VAULT.with(|v| {
        let mut v = v.borrow_mut();
        let Some(vault) = v.as_mut() else {
            return error_answer(&lockdown_app::AppError::refused(
                "the vault isn't started; call ld_init first",
            ));
        };
        match call(vault, &cmd, args) {
            Ok(Reply::Json(value)) => {
                answer(0, &serde_json::to_vec(&value).unwrap_or_else(|_| b"null".to_vec()))
            }
            Ok(Reply::Bytes(bytes)) => answer(1, &bytes),
            Err(e) => error_answer(&e),
        }
    })
}

/// The HTTP hook: the request goes to the worker, the answer comes back.
fn send(req: &HttpRequest<'_>) -> Result<(u16, Vec<u8>), String> {
    let meta = serde_json::json!({
        "method": req.method.as_str(),
        "url": req.url,
        "contentType": req.content_type,
        "authorization": req.authorization,
    });
    let meta = serde_json::to_vec(&meta).map_err(|e| e.to_string())?;
    let body = req.body.unwrap_or(&[]);
    let mut frame = Vec::with_capacity(4 + meta.len() + body.len());
    frame.extend_from_slice(&u32::try_from(meta.len()).map_err(|e| e.to_string())?.to_le_bytes());
    frame.extend_from_slice(&meta);
    frame.extend_from_slice(body);
    // SAFETY: `frame` is alive and `frame.len()` bytes long for the call.
    let held = unsafe { http_send(frame.as_ptr(), frame.len()) } as usize;
    frame.zeroize();
    if held < 2 {
        return Err("the browser gave no answer".into());
    }
    let mut answer = vec![0u8; held];
    // SAFETY: the worker holds exactly `held` bytes and copies them here.
    unsafe { http_take(answer.as_mut_ptr()) };
    let status = u16::from_le_bytes([answer[0], answer[1]]);
    let body = answer.split_off(2);
    if status == 0 {
        return Err(String::from_utf8_lossy(&body).into_owned());
    }
    Ok((status, body))
}
