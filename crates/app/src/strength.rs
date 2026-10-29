//! How hard a password the person is typing would be to guess (D116): the
//! zxcvbn estimate (dictionaries, keyboard walks, dates, repeats, l33t
//! swaps), as its 0–4 score. unicron's `secretStrengthFromScore` turns the
//! score into the editor's word (0–2 weak, 3 fair, 4 strong), so every
//! shell says the same word for the same password.
//!
//! Only ever a candidate the person just typed into an editor: a stored
//! secret is never scored, on reveal or anywhere else, so nothing here
//! reads the vault. The candidate arrives as bytes a shell wipes; the copy
//! made here for the estimate is wiped when the call returns. zxcvbn's own
//! working strings (the pieces it matched) are ordinary heap strings freed
//! without wiping, which is why the input is held to [`MAX_SCORED`]
//! characters and the function is never called on anything stored.

use zeroize::Zeroizing;

/// The longest prefix scored, in characters. Past this zxcvbn's work grows
/// quickly and adds nothing: a hundred characters typed by hand score 4
/// unless they repeat, and a repeat shows in the prefix.
pub const MAX_SCORED: usize = 100;

/// The words beside a password that make it easier to guess when it
/// contains them (the item's title, username, website), at most this many.
const MAX_CONTEXT: usize = 8;

/// The 0–4 guess score of `candidate`, or `None` for an empty one (no
/// meter is drawn). `context` holds words from the same form, which count
/// as guessable when the candidate contains them. A candidate that isn't
/// UTF-8 is refused: a password field only ever sends text's bytes.
pub fn score(candidate: &[u8], context: &[&str]) -> Result<Option<u8>, String> {
    let text = std::str::from_utf8(candidate).map_err(|_| "this password isn't text")?;
    if text.is_empty() {
        return Ok(None);
    }
    let scored: Zeroizing<String> = Zeroizing::new(text.chars().take(MAX_SCORED).collect());
    let context: Vec<&str> = context
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .take(MAX_CONTEXT)
        .collect();
    let estimate = zxcvbn::zxcvbn(&scored, &context);
    Ok(Some(u8::from(estimate.score())))
}

/// The fewest bits a generated password needs to read "fair" (D230 as
/// changed by D375).
pub const FAIR_FROM_BITS: f64 = 50.0;

/// The fewest bits a generated password needs to read "strong": the
/// default six-word passphrase (77.5 bits) clears it.
pub const STRONG_FROM_BITS: f64 = 75.0;

/// The word for a generated password of `bits` of entropy, the one place
/// that decides it (D230 as changed by D375): under 50 bits weak, 50 to
/// under 75 fair, from 75 strong. The generator's preview carries it to
/// every shell (the desktop's and the web vault's editors, the
/// extension's suggestion), so none of them decides on its own.
pub fn bits_word(bits: f64) -> &'static str {
    if bits < FAIR_FROM_BITS {
        "weak"
    } else if bits < STRONG_FROM_BITS {
        "fair"
    } else {
        "strong"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(candidate: &str) -> u8 {
        score(candidate.as_bytes(), &[]).unwrap().unwrap()
    }

    #[test]
    fn common_and_patterned_passwords_score_low_and_random_ones_high() {
        assert_eq!(of("password"), 0);
        assert_eq!(of("qwerty123"), 0);
        // A character-class count would call this strong; zxcvbn sees the word.
        assert!(of("P@ssw0rd1") <= 1, "{}", of("P@ssw0rd1"));
        assert!(of("correct horse") <= 3);
        // The core's own strong rule (D112): twenty letters, digits and symbols.
        assert_eq!(of("k7#Qm2^vX9@pL4&wR8!z"), 4);
        assert_eq!(of("correct horse battery staple"), 4);
    }

    #[test]
    fn words_from_the_same_form_make_a_password_weaker() {
        // A surname and a year: strong alone, weak beside the item's username.
        assert_eq!(of("Quintavalle1987"), 4);
        let beside = score(b"Quintavalle1987", &["quintavalle", " "])
            .unwrap()
            .unwrap();
        assert!(beside <= 1, "{beside}");
    }

    #[test]
    fn empty_is_no_score_and_bytes_that_are_not_text_are_refused() {
        assert_eq!(score(b"", &[]).unwrap(), None);
        assert!(score(&[0xff, 0xfe], &[]).is_err());
        // A long paste is scored on its first hundred characters, quickly.
        let long = "ab".repeat(5_000);
        assert!(score(long.as_bytes(), &[]).unwrap().unwrap() <= 2);
    }

    #[test]
    fn the_strength_word_turns_at_fifty_and_seventy_five_bits() {
        assert_eq!(bits_word(0.0), "weak");
        assert_eq!(bits_word(49.9), "weak");
        assert_eq!(bits_word(50.0), "fair");
        assert_eq!(bits_word(74.9), "fair");
        assert_eq!(bits_word(75.0), "strong");
        // The default passphrase: six words from EFF's long list.
        assert_eq!(bits_word(6.0 * 7776f64.log2()), "strong");
        assert_eq!(bits_word(5.0 * 7776f64.log2()), "fair");
        assert_eq!(bits_word(129.0), "strong");
    }
}
