//! QR codes for the shells to draw (docs/plan/22-security-depth.md §2):
//! the module grid of a text, as rows of `0` and `1`, so a shell draws it
//! with its own elements and no image or markup crosses the bridge.

use serde::Serialize;

/// A QR code's modules, row by row (`1` is dark). Empty when the text is
/// too long for a QR code.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct QrRows(pub Vec<String>);

/// The QR code of `text` at error correction level M.
pub fn rows(text: &str) -> QrRows {
    let Ok(code) = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)
    else {
        return QrRows::default();
    };
    let width = code.width();
    QrRows(
        code.to_colors()
            .chunks(width)
            .map(|row| {
                row.iter()
                    .map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' })
                    .collect()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_payload_becomes_a_square_grid_with_finder_patterns() {
        let r = rows(
            "lockdown:link:v1?s=https%3A%2F%2Fvault.example&c=ABCDE12345&f=00112233445566778899aabbccddeeff",
        );
        let w = r.0.len();
        assert!(w >= 21 && (w - 21) % 4 == 0, "a QR code's side is 21 + 4k");
        assert!(r.0.iter().all(|row| row.len() == w));
        // The top-left finder pattern: seven dark modules across the top.
        assert!(r.0[0].starts_with("1111111"));
        assert!(rows(&"x".repeat(5000)).0.is_empty());
    }
}
