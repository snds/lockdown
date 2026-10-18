//! The camera picture the web vault's browser test points Chromium's fake
//! camera at (`apps/web/e2e/fixtures/tv-code.y4m`, docs/plan/26-desktop-finish.md
//! §3): a TV's activation QR in a YUV4MPEG2 frame. This test makes it from
//! the text with the same QR encoder the app uses, checks the committed file
//! is exactly that, and reads it back through the core's own decoder.
//! `LOCKDOWN_WRITE_FIXTURES=1 cargo test -p lockdown-app --test scan_fixture`
//! rewrites it.

use lockdown_app::scan::decode_qr;

/// What the TV in the browser test shows.
pub const TV_CODE: &str = "https://www.youtube.com/activate?user_code=WDJB-MJHT";
const W: usize = 176;
const H: usize = 144;

fn luma_of(text: &str) -> Vec<u8> {
    let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
    let n = code.width();
    let colors = code.to_colors();
    let scale = 3;
    let side = (n + 8) * scale;
    assert!(side <= H, "the code fits the frame");
    let (left, top) = ((W - side) / 2, (H - side) / 2);
    let mut luma = vec![255u8; W * H];
    for y in 0..side {
        for x in 0..side {
            let (mx, my) = (x / scale, y / scale);
            if (4..n + 4).contains(&mx)
                && (4..n + 4).contains(&my)
                && colors[(my - 4) * n + (mx - 4)] == qrcode::Color::Dark
            {
                luma[(top + y) * W + left + x] = 16;
            }
        }
    }
    luma
}

fn y4m(luma: &[u8]) -> Vec<u8> {
    let mut out = format!("YUV4MPEG2 W{W} H{H} F1:1 Ip A1:1 C420jpeg\nFRAME\n").into_bytes();
    out.extend_from_slice(luma);
    out.extend(std::iter::repeat_n(128u8, (W / 2) * (H / 2) * 2));
    out
}

#[test]
fn the_fake_camera_s_picture_is_current_and_reads_back() {
    let luma = luma_of(TV_CODE);
    assert_eq!(
        decode_qr(W as u32, H as u32, &luma).as_deref(),
        Some(TV_CODE)
    );
    let want = y4m(&luma);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/e2e/fixtures/tv-code.y4m");
    if std::env::var_os("LOCKDOWN_WRITE_FIXTURES").is_some() {
        std::fs::write(&path, &want).unwrap();
    }
    let have = std::fs::read(&path).expect("the fixture exists");
    assert!(
        have == want,
        "tv-code.y4m is stale; rerun with LOCKDOWN_WRITE_FIXTURES=1"
    );
}
