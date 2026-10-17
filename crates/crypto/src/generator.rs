//! The password generator (03-architecture §2 `crypto` row; matrix
//! `password-generator`). Every character is drawn from the operating
//! system's generator ([`crate::rand`]) by rejection sampling, so each one
//! is uniform over its alphabet; nothing is seeded from the clock or a
//! user-space generator (brief §4.9 "honest generators").
//!
//! A rule names the length and which classes of character it uses. Every
//! class a rule turns on appears at least once in what it makes (sites
//! that demand "one digit, one symbol" accept it first time); a draw that
//! misses one is thrown away whole and drawn again, so the result stays
//! uniform over every password that meets the rule.

use zeroize::Zeroizing;

use crate::rand::random_bytes;
use crate::{Error, Result};

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
/// Symbols every common site accepts: no quotes, backslash, space or angle
/// brackets, which some forms reject or mangle.
const SYMBOLS: &str = "!#$%&*+-=?@^_~";
/// Characters that read alike across a room or on a TV keyboard (0 and O,
/// 1, l and I, 5 and S, 2 and Z, 8 and B), left out by an unambiguous rule.
const AMBIGUOUS: &str = "0O1lI5S2Z8B";

/// The shortest and longest passwords a rule may ask for.
pub const MIN_LENGTH: usize = 8;
/// See [`MIN_LENGTH`].
pub const MAX_LENGTH: usize = 128;

/// What to make.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
pub struct PasswordRule {
    /// How many characters, [`MIN_LENGTH`] to [`MAX_LENGTH`].
    pub length: usize,
    /// Upper- and lower-case letters (always on: a password is never digits only).
    pub letters: bool,
    /// The digits.
    pub digits: bool,
    /// The symbols in `SYMBOLS`.
    pub symbols: bool,
    /// Leave out characters that read alike (for large type and TV keyboards).
    pub unambiguous: bool,
}

impl PasswordRule {
    /// The default for an account password: twenty characters, letters,
    /// digits and symbols from 76 (about 125 bits).
    pub const STRONG: Self = Self {
        length: 20,
        letters: true,
        digits: true,
        symbols: true,
        unambiguous: false,
    };

    /// For a password people read and type by hand (a Wi-Fi network on a
    /// TV): twenty letters and digits with the look-alikes left out
    /// from 51 (about 113 bits).
    pub const READABLE: Self = Self {
        length: 20,
        letters: true,
        digits: true,
        symbols: false,
        unambiguous: true,
    };

    /// The rule in plain words, for the control that offers it.
    pub fn describe(&self) -> String {
        let mut what = vec!["letters"];
        if self.digits {
            what.push("digits");
        }
        if self.symbols {
            what.push("symbols");
        }
        let list = match what.as_slice() {
            [one] => (*one).to_string(),
            [init @ .., last] => format!("{} and {last}", init.join(", ")),
            [] => String::new(),
        };
        let tail = if self.unambiguous {
            ", none that look alike"
        } else {
            ""
        };
        format!("{} characters: {list}{tail}", self.length)
    }

    /// The classes this rule uses, each as its alphabet.
    fn classes(&self) -> Vec<Vec<u8>> {
        let keep = |set: &str| -> Vec<u8> {
            set.bytes()
                .filter(|b| !(self.unambiguous && AMBIGUOUS.as_bytes().contains(b)))
                .collect()
        };
        let mut out = vec![keep(LOWER), keep(UPPER)];
        if self.digits {
            out.push(keep(DIGITS));
        }
        if self.symbols {
            out.push(keep(SYMBOLS));
        }
        out
    }
}

/// One index below `n`, uniform: a random byte is drawn again whenever it
/// lands in the uneven tail above the largest multiple of `n`.
fn uniform_below(n: usize) -> Result<usize> {
    debug_assert!(n > 0 && n <= 256);
    let limit = 256 - (256 % n);
    loop {
        let mut b = [0u8; 1];
        random_bytes(&mut b)?;
        let v = usize::from(b[0]);
        if v < limit {
            return Ok(v % n);
        }
    }
}

/// Makes one password under `rule`. The text is wiped when dropped.
pub fn generate_password(rule: &PasswordRule) -> Result<Zeroizing<String>> {
    if !(MIN_LENGTH..=MAX_LENGTH).contains(&rule.length) {
        return Err(Error::Invalid(format!(
            "a generated password is {MIN_LENGTH} to {MAX_LENGTH} characters, not {}",
            rule.length
        )));
    }
    if !rule.letters {
        return Err(Error::Invalid(
            "a generated password always has letters".into(),
        ));
    }
    let classes = rule.classes();
    let alphabet: Vec<u8> = classes.iter().flatten().copied().collect();
    // The shortest four-class rule (8 characters) misses a class a little
    // over half the time and the defaults about one draw in eight; a
    // thousand misses in a row does not happen with a working source, and
    // the bound stops a broken one spinning forever.
    for _ in 0..1000 {
        let mut out = Zeroizing::new(String::with_capacity(rule.length));
        for _ in 0..rule.length {
            out.push(char::from(alphabet[uniform_below(alphabet.len())?]));
        }
        if classes
            .iter()
            .all(|set| out.bytes().any(|b| set.contains(&b)))
        {
            return Ok(out);
        }
    }
    Err(Error::Random)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_strong_password_has_every_class_and_nothing_else() {
        for _ in 0..200 {
            let pw = generate_password(&PasswordRule::STRONG).unwrap();
            assert_eq!(pw.len(), 20);
            assert!(pw.bytes().any(|b| b.is_ascii_lowercase()));
            assert!(pw.bytes().any(|b| b.is_ascii_uppercase()));
            assert!(pw.bytes().any(|b| b.is_ascii_digit()));
            assert!(pw.bytes().any(|b| SYMBOLS.as_bytes().contains(&b)));
            assert!(
                pw.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || SYMBOLS.as_bytes().contains(&b))
            );
        }
    }

    #[test]
    fn a_readable_password_leaves_out_symbols_and_look_alikes() {
        for _ in 0..200 {
            let pw = generate_password(&PasswordRule::READABLE).unwrap();
            assert_eq!(pw.len(), 20);
            assert!(pw.bytes().all(|b| b.is_ascii_alphanumeric()));
            assert!(!pw.bytes().any(|b| AMBIGUOUS.as_bytes().contains(&b)));
            assert!(pw.bytes().any(|b| b.is_ascii_digit()));
        }
    }

    #[test]
    fn two_passwords_differ_and_bad_rules_are_refused() {
        let a = generate_password(&PasswordRule::STRONG).unwrap();
        let b = generate_password(&PasswordRule::STRONG).unwrap();
        assert_ne!(*a, *b);
        let short = PasswordRule {
            length: 4,
            ..PasswordRule::STRONG
        };
        assert!(generate_password(&short).is_err());
        let long = PasswordRule {
            length: MAX_LENGTH + 1,
            ..PasswordRule::STRONG
        };
        assert!(generate_password(&long).is_err());
        let digits_only = PasswordRule {
            letters: false,
            ..PasswordRule::STRONG
        };
        assert!(generate_password(&digits_only).is_err());
        // The shortest four-class rule still meets every class.
        let tight = PasswordRule {
            length: MIN_LENGTH,
            ..PasswordRule::STRONG
        };
        for _ in 0..200 {
            assert_eq!(generate_password(&tight).unwrap().len(), MIN_LENGTH);
        }
    }

    /// A coarse uniformity check over the whole alphabet: with 76 symbols
    /// and 76 000 draws each is expected 1 000 times; a chi-squared above
    /// 150 on 75 degrees of freedom has odds far below one in a million
    /// for a fair source, while a modulo-biased draw fails it.
    #[test]
    fn characters_are_uniform_over_the_alphabet() {
        let rule = PasswordRule {
            length: 100,
            ..PasswordRule::STRONG
        };
        let alphabet: Vec<u8> = rule.classes().into_iter().flatten().collect();
        let mut counts = std::collections::HashMap::<u8, f64>::new();
        let mut total = 0.0;
        for _ in 0..760 {
            for b in generate_password(&rule).unwrap().bytes() {
                *counts.entry(b).or_default() += 1.0;
                total += 1.0;
            }
        }
        let expected = total / alphabet.len() as f64;
        let chi: f64 = alphabet
            .iter()
            .map(|b| {
                let seen = counts.get(b).copied().unwrap_or(0.0);
                (seen - expected).powi(2) / expected
            })
            .sum();
        assert!(
            chi < 150.0,
            "chi-squared {chi} over {} symbols",
            alphabet.len()
        );
    }

    #[test]
    fn the_rule_reads_in_plain_words() {
        assert_eq!(
            PasswordRule::STRONG.describe(),
            "20 characters: letters, digits and symbols"
        );
        assert_eq!(
            PasswordRule::READABLE.describe(),
            "20 characters: letters and digits, none that look alike"
        );
    }
}
