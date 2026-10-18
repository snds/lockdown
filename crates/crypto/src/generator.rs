//! The password generator (03-architecture §2 `crypto` row; matrix
//! `password-generator`). Every character and every word is drawn from the
//! operating system's generator ([`crate::rand`]) by rejection sampling, so
//! each one is uniform over its alphabet or its word list; nothing is
//! seeded from the clock or a user-space generator (brief §4.9 "honest
//! generators").
//!
//! Three shapes of rule, one engine:
//!
//! - [`PasswordRule`]: the two fixed rules the templates name (D112),
//!   `STRONG` and `READABLE`.
//! - [`CharacterRule`]: any character password, as sets that must each
//!   appear at least once plus characters that may, a length and an
//!   optional cap on runs of one character. A [`PasswordRule`], the
//!   person's options ([`Recipe::Characters`]) and a site's published
//!   rules ([`SiteRule`], Apple's `passwordrules` language) all lower to
//!   one.
//! - [`PassphraseRule`]: words from EFF's long word list (7,776 words, CC
//!   BY 3.0 US; `data/eff_large_wordlist.LICENSE`), about 12.9 bits each.
//!
//! Every set a rule requires appears at least once in what it makes (sites
//! that demand "one digit, one symbol" accept it first time); a draw that
//! misses one, or breaks the run cap, is thrown away whole and drawn again,
//! so the result stays uniform over every password that meets the rule.
//! The statistical tests at the bottom run in CI (`cargo test -p
//! lockdown-crypto generator`): chi-squared over characters, positions,
//! neighbouring pairs and words, and a monobit and runs test of the source.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::rand::random_bytes;
use crate::{Error, Result};

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
/// Symbols every common site accepts: no quotes, backslash, space or angle
/// brackets, which some forms reject or mangle.
pub const SYMBOLS: &str = "!#$%&*+-=?@^_~";
/// Characters that read alike across a room or on a TV keyboard (0 and O,
/// 1, l and I, 5 and S, 2 and Z, 8 and B), left out by an unambiguous rule.
pub const AMBIGUOUS: &str = "0O1lI5S2Z8B";

/// The shortest and longest passwords a person's rule may ask for.
pub const MIN_LENGTH: usize = 8;
/// See [`MIN_LENGTH`].
pub const MAX_LENGTH: usize = 128;
/// The shortest password a site's own published rule may force (some
/// banks cap a password at six). Shorter is refused.
pub const SITE_MIN_LENGTH: usize = 4;
/// The fewest and most words in a passphrase.
pub const MIN_WORDS: usize = 3;
/// See [`MIN_WORDS`].
pub const MAX_WORDS: usize = 20;

/// EFF's long word list, one word per line (`data/eff_large_wordlist.txt`).
const WORD_LIST: &str = include_str!("../data/eff_large_wordlist.txt");

/// The word list, in its own order.
pub fn words() -> &'static [&'static str] {
    static WORDS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| WORD_LIST.lines().filter(|l| !l.is_empty()).collect())
}

/// The bits a passphrase word carries: log2 of the list's length.
pub fn bits_per_word() -> f64 {
    (words().len() as f64).log2()
}

/// What to make, as the templates name it (D112).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
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
        let tail = if self.unambiguous {
            ", none that look alike"
        } else {
            ""
        };
        format!("{} characters: {}{tail}", self.length, list(&what))
    }

    /// The rule as a [`CharacterRule`].
    pub fn to_character_rule(&self) -> CharacterRule {
        let keep = |set: &str| -> Vec<u8> {
            set.bytes()
                .filter(|b| !(self.unambiguous && AMBIGUOUS.as_bytes().contains(b)))
                .collect()
        };
        let mut required = vec![keep(LOWER), keep(UPPER)];
        if self.digits {
            required.push(keep(DIGITS));
        }
        if self.symbols {
            required.push(keep(SYMBOLS));
        }
        CharacterRule {
            length: self.length,
            required,
            allowed: Vec::new(),
            max_consecutive: None,
        }
    }

    /// How many bits of entropy a password under this rule carries.
    pub fn entropy_bits(&self) -> f64 {
        self.to_character_rule().entropy_bits()
    }
}

fn list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_string(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// Any character password: each set in `required` appears at least once,
/// and every character comes from those sets or `allowed`. ASCII only.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CharacterRule {
    /// How many characters.
    pub length: usize,
    /// Sets that must each appear at least once.
    pub required: Vec<Vec<u8>>,
    /// Characters that may appear beyond the required sets.
    pub allowed: Vec<u8>,
    /// No run of one character longer than this.
    pub max_consecutive: Option<usize>,
}

impl CharacterRule {
    /// Every character the rule may draw, sorted, each once.
    pub fn alphabet(&self) -> Vec<u8> {
        let mut out: Vec<u8> = self
            .required
            .iter()
            .flatten()
            .chain(self.allowed.iter())
            .copied()
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The bits of a uniform draw over the alphabet at this length (the
    /// at-least-one-of-each rule removes a small share of those draws, so
    /// this is a slight overestimate; under one bit for every rule here).
    pub fn entropy_bits(&self) -> f64 {
        let n = self.alphabet().len();
        if n < 2 {
            return 0.0;
        }
        self.length as f64 * (n as f64).log2()
    }

    /// Takes `avoid`'s characters out of every set; a required set left
    /// empty is refused (the rule could not be met).
    pub fn without(mut self, avoid: &str) -> Result<Self> {
        if avoid.is_empty() {
            return Ok(self);
        }
        let avoid = avoid.as_bytes();
        for set in &mut self.required {
            set.retain(|b| !avoid.contains(b));
            if set.is_empty() {
                return Err(Error::Invalid(
                    "leaving those characters out leaves a kind of character the password needs with nothing to draw from".into(),
                ));
            }
        }
        self.allowed.retain(|b| !avoid.contains(b));
        Ok(self)
    }

    fn check(&self, floor: usize) -> Result<()> {
        if !(floor..=MAX_LENGTH).contains(&self.length) {
            return Err(Error::Invalid(format!(
                "a generated password is {floor} to {MAX_LENGTH} characters, not {}",
                self.length
            )));
        }
        if self.alphabet().len() < 2 {
            return Err(Error::Invalid(
                "a generated password needs at least two characters to draw from".into(),
            ));
        }
        if self.required.iter().any(|s| s.is_empty()) {
            return Err(Error::Invalid(
                "every kind of character a password needs must have something in it".into(),
            ));
        }
        if self.required.len() > self.length {
            return Err(Error::Invalid(
                "the password is too short to hold every kind of character it needs".into(),
            ));
        }
        if !self.alphabet().iter().all(|b| b.is_ascii_graphic()) {
            return Err(Error::Invalid(
                "a generated password uses printable characters only, no spaces".into(),
            ));
        }
        Ok(())
    }

    fn meets(&self, out: &[u8]) -> bool {
        if !self
            .required
            .iter()
            .all(|set| out.iter().any(|b| set.contains(b)))
        {
            return false;
        }
        if let Some(cap) = self.max_consecutive {
            let mut run = 0;
            let mut last = None;
            for b in out {
                run = if Some(b) == last { run + 1 } else { 1 };
                last = Some(b);
                if run > cap {
                    return false;
                }
            }
        }
        true
    }

    /// Makes one password. The text is wiped when dropped.
    pub fn generate(&self) -> Result<Zeroizing<String>> {
        self.generate_from(MIN_LENGTH)
    }

    fn generate_from(&self, floor: usize) -> Result<Zeroizing<String>> {
        self.check(floor)?;
        let alphabet = self.alphabet();
        // A draw misses a required set a little over half the time at the
        // shortest four-set rule and about one in eight at the defaults; a
        // site's narrow set ("[!#]" of 70) can miss most of the time. Ten
        // thousand misses in a row does not happen with a working source,
        // and the bound stops a broken one spinning forever.
        for _ in 0..10_000 {
            let mut out = Zeroizing::new(Vec::with_capacity(self.length));
            for _ in 0..self.length {
                out.push(alphabet[uniform_below(alphabet.len())?]);
            }
            if self.meets(&out) {
                let text = String::from_utf8(out.to_vec()).map_err(|_| Error::Random)?;
                return Ok(Zeroizing::new(text));
            }
        }
        Err(Error::Random)
    }
}

/// A site's published password rules, in the `passwordrules` language
/// (Apple's password-manager-resources; WHATWG proposal): `minlength`,
/// `maxlength`, `max-consecutive`, `required` and `allowed`, each a list
/// of the classes `upper`, `lower`, `digit`, `special`, `ascii-printable`,
/// `unicode` and `[custom characters]`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SiteRule {
    /// `minlength`.
    pub min_length: Option<usize>,
    /// `maxlength`.
    pub max_length: Option<usize>,
    /// `max-consecutive`.
    pub max_consecutive: Option<usize>,
    /// Each `required` line: one of these characters must appear.
    pub required: Vec<Vec<u8>>,
    /// The `allowed` lines together.
    pub allowed: Vec<u8>,
}

/// The characters a named class draws from when Lockdown generates. The
/// `special` and printable classes draw from [`SYMBOLS`], every one of
/// which they contain, so a generated password never holds a quote, a
/// space or a backslash a form might mangle; a site's own `[…]` set is
/// used as written.
fn class_chars(name: &str) -> Option<Vec<u8>> {
    let all = |s: &[&str]| s.iter().flat_map(|c| c.bytes()).collect::<Vec<u8>>();
    Some(match name {
        "upper" => UPPER.bytes().collect(),
        "lower" => LOWER.bytes().collect(),
        "digit" => DIGITS.bytes().collect(),
        "special" => SYMBOLS.bytes().collect(),
        "ascii-printable" | "unicode" => all(&[LOWER, UPPER, DIGITS, SYMBOLS]),
        _ => return None,
    })
}

impl SiteRule {
    /// Reads a rule. Unknown properties are skipped (the language grows);
    /// a malformed value is refused rather than guessed at.
    pub fn parse(text: &str) -> Result<Self> {
        let bad = |why: String| Error::Invalid(format!("the site's password rule {why}"));
        let mut rule = SiteRule::default();
        for part in split_properties(text) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (name, value) = part
                .split_once(':')
                .ok_or_else(|| bad(format!("has '{part}', which isn't 'name: value'")))?;
            let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
            let number = || {
                value
                    .parse::<usize>()
                    .map_err(|_| bad(format!("gives {name} as '{value}', not a number")))
            };
            match name.as_str() {
                "minlength" => rule.min_length = Some(number()?),
                "maxlength" => rule.max_length = Some(number()?),
                "max-consecutive" => rule.max_consecutive = Some(number()?),
                "required" => {
                    let set = parse_classes(value).map_err(bad)?;
                    rule.required.push(set);
                }
                "allowed" => {
                    let set = parse_classes(value).map_err(bad)?;
                    rule.allowed.extend(set);
                }
                _ => {}
            }
        }
        if let (Some(min), Some(max)) = (rule.min_length, rule.max_length)
            && min > max
        {
            return Err(bad(format!("asks for at least {min} and at most {max}")));
        }
        rule.allowed.sort_unstable();
        rule.allowed.dedup();
        Ok(rule)
    }

    /// The rule back in its own language, for keeping on a login.
    pub fn to_text(&self) -> String {
        let mut out = Vec::new();
        if let Some(n) = self.min_length {
            out.push(format!("minlength: {n};"));
        }
        if let Some(n) = self.max_length {
            out.push(format!("maxlength: {n};"));
        }
        if let Some(n) = self.max_consecutive {
            out.push(format!("max-consecutive: {n};"));
        }
        let class = |set: &[u8]| {
            for (name, chars) in [("lower", LOWER), ("upper", UPPER), ("digit", DIGITS)] {
                if set == chars.as_bytes() {
                    return name.to_string();
                }
            }
            if set == SYMBOLS.as_bytes() {
                return "special".into();
            }
            // `]` first and `-` last read unambiguously inside a set.
            let mut s: Vec<u8> = set.to_vec();
            s.sort_unstable();
            let mut text = String::from("[");
            if s.contains(&b']') {
                text.push(']');
            }
            for b in s.iter().filter(|b| **b != b']' && **b != b'-') {
                text.push(char::from(*b));
            }
            if s.contains(&b'-') {
                text.push('-');
            }
            text.push(']');
            text
        };
        for set in &self.required {
            out.push(format!("required: {};", class(set)));
        }
        if !self.allowed.is_empty() {
            out.push(format!("allowed: {};", class(&self.allowed)));
        }
        out.join(" ")
    }

    /// The character rule for a password of about `length` characters
    /// under this site's rules: the length is moved inside the site's
    /// bounds, the sets are the site's (letters and digits when it names
    /// none), and `avoid`'s characters are left out.
    pub fn character_rule(&self, length: usize, avoid: &str) -> Result<CharacterRule> {
        let mut length = length;
        if let Some(min) = self.min_length {
            length = length.max(min);
        }
        if let Some(max) = self.max_length {
            length = length.min(max);
        }
        let mut required = self.required.clone();
        if required.is_empty() && self.allowed.is_empty() {
            required = vec![
                LOWER.bytes().collect(),
                UPPER.bytes().collect(),
                DIGITS.bytes().collect(),
            ];
        }
        CharacterRule {
            length,
            required,
            allowed: self.allowed.clone(),
            max_consecutive: self.max_consecutive,
        }
        .without(avoid)
    }

    /// One password under this site's rules, about `length` long.
    pub fn generate(&self, length: usize, avoid: &str) -> Result<Zeroizing<String>> {
        self.character_rule(length, avoid)?
            .generate_from(SITE_MIN_LENGTH)
    }

    /// The rule in plain words.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        match (self.min_length, self.max_length) {
            (Some(a), Some(b)) => parts.push(format!("{a} to {b} characters")),
            (Some(a), None) => parts.push(format!("at least {a} characters")),
            (None, Some(b)) => parts.push(format!("at most {b} characters")),
            (None, None) => {}
        }
        if !self.required.is_empty() {
            let names: Vec<String> = self
                .required
                .iter()
                .map(|s| {
                    if s == LOWER.as_bytes() {
                        "a lower-case letter".into()
                    } else if s == UPPER.as_bytes() {
                        "a capital".into()
                    } else if s == DIGITS.as_bytes() {
                        "a digit".into()
                    } else if s == SYMBOLS.as_bytes() {
                        "a symbol".into()
                    } else {
                        let chars: String = s.iter().map(|b| char::from(*b)).collect();
                        format!("one of {chars}")
                    }
                })
                .collect();
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            parts.push(format!("with {}", list(&names)));
        }
        if let Some(n) = self.max_consecutive {
            parts.push(format!("no character more than {n} times in a row"));
        }
        if parts.is_empty() {
            "no rules of its own".into()
        } else {
            parts.join(", ")
        }
    }
}

/// Splits on `;` outside a `[…]` set.
fn split_properties(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_set = false;
    let mut set_start = false;
    for c in text.chars() {
        match c {
            '[' if !in_set => {
                in_set = true;
                set_start = true;
                cur.push(c);
                continue;
            }
            ']' if in_set && !set_start => in_set = false,
            ';' if !in_set => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        set_start = false;
        cur.push(c);
    }
    out.push(cur);
    out
}

fn parse_classes(value: &str) -> std::result::Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut rest = value.trim();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('[') {
            // A `]` straight after the `[` is a member, not the end.
            // So is a `]` straight before the closing one (`[…]]`), the
            // way the published rules write a set ending in `]`.
            let skip = usize::from(after.starts_with(']'));
            let mut end = after[skip..]
                .find(']')
                .ok_or_else(|| format!("has a set that never closes: '{rest}'"))?
                + skip;
            while after[end + 1..].starts_with(']') {
                end += 1;
            }
            for c in after[..end].chars() {
                if !c.is_ascii() || c.is_ascii_control() {
                    return Err("has a character outside printable ASCII in a set".into());
                }
                // A space is never drawn, even where a site allows it.
                if c != ' ' {
                    out.push(c as u8);
                }
            }
            rest = after[end + 1..].trim_start();
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            let name = rest[..end].trim().to_ascii_lowercase();
            let chars = class_chars(&name)
                .ok_or_else(|| format!("names a class '{name}' it doesn't know"))?;
            out.extend(chars);
            rest = &rest[end..];
        }
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix(',') {
            rest = r.trim_start();
        }
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err("has a class with nothing in it".into());
    }
    Ok(out)
}

/// A passphrase: words from EFF's long list.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PassphraseRule {
    /// How many words, [`MIN_WORDS`] to [`MAX_WORDS`].
    pub words: usize,
    /// What goes between words: one printable character that is not a
    /// letter or a digit, or a space.
    pub separator: String,
    /// Each word starts with a capital.
    pub capitalize: bool,
    /// One digit at the end of one word, both chosen at random.
    pub number: bool,
}

impl Default for PassphraseRule {
    fn default() -> Self {
        Self {
            words: 6,
            separator: "-".into(),
            capitalize: false,
            number: false,
        }
    }
}

impl PassphraseRule {
    fn check(&self) -> Result<()> {
        if !(MIN_WORDS..=MAX_WORDS).contains(&self.words) {
            return Err(Error::Invalid(format!(
                "a passphrase is {MIN_WORDS} to {MAX_WORDS} words, not {}",
                self.words
            )));
        }
        let mut chars = self.separator.chars();
        let ok = match (chars.next(), chars.next()) {
            (Some(c), None) => c == ' ' || (c.is_ascii_graphic() && !c.is_ascii_alphanumeric()),
            _ => false,
        };
        if !ok {
            return Err(Error::Invalid(
                "words are kept apart by one space or one symbol, never a letter or a digit".into(),
            ));
        }
        Ok(())
    }

    /// The bits a passphrase under this rule carries: the words' bits,
    /// and, with a number, the digit and the word it went on. A capital on
    /// every word adds none.
    pub fn entropy_bits(&self) -> f64 {
        let mut bits = self.words as f64 * bits_per_word();
        if self.number {
            bits += (10.0 * self.words as f64).log2();
        }
        bits
    }

    /// The rule in plain words.
    pub fn describe(&self) -> String {
        let sep = match self.separator.as_str() {
            " " => "spaces".to_string(),
            s => format!("\"{s}\""),
        };
        let mut s = format!("{} words joined by {sep}", self.words);
        if self.capitalize {
            s.push_str(", each with a capital");
        }
        if self.number {
            s.push_str(", one with a digit");
        }
        s
    }

    /// Makes one passphrase. The text is wiped when dropped.
    pub fn generate(&self) -> Result<Zeroizing<String>> {
        self.check()?;
        let list = words();
        let mut chosen: Vec<Zeroizing<String>> = Vec::with_capacity(self.words);
        for _ in 0..self.words {
            let word = list[uniform_below_wide(list.len())?];
            let mut w = Zeroizing::new(String::with_capacity(word.len() + 1));
            if self.capitalize {
                let mut cs = word.chars();
                if let Some(first) = cs.next() {
                    w.push(first.to_ascii_uppercase());
                }
                w.push_str(cs.as_str());
            } else {
                w.push_str(word);
            }
            chosen.push(w);
        }
        if self.number {
            let at = uniform_below_wide(self.words)?;
            let digit = uniform_below(10)?;
            chosen[at].push(char::from(b'0' + digit as u8));
        }
        let mut out = Zeroizing::new(String::new());
        for (i, w) in chosen.iter().enumerate() {
            if i > 0 {
                out.push_str(&self.separator);
            }
            out.push_str(w);
        }
        Ok(out)
    }
}

/// What a shell asks the generator for: the person's options in the
/// editor (D230). Serialized as `{"style": "characters", …}` or
/// `{"style": "words", …}`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "style", rename_all = "snake_case")]
pub enum Recipe {
    /// A character password: letters always, digits and symbols by choice.
    Characters {
        /// How many characters.
        length: usize,
        /// Digits in it.
        digits: bool,
        /// Symbols from [`SYMBOLS`] in it.
        symbols: bool,
        /// Leave out look-alikes ([`AMBIGUOUS`]).
        #[serde(default)]
        unambiguous: bool,
        /// Characters to leave out as well.
        #[serde(default)]
        avoid: String,
    },
    /// A passphrase.
    Words {
        /// How many words.
        words: usize,
        /// What goes between them.
        #[serde(default = "default_separator")]
        separator: String,
        /// A capital on each word.
        #[serde(default)]
        capitalize: bool,
        /// One digit on one word.
        #[serde(default)]
        number: bool,
    },
}

fn default_separator() -> String {
    "-".into()
}

impl From<PasswordRule> for Recipe {
    fn from(r: PasswordRule) -> Self {
        Self::Characters {
            length: r.length,
            digits: r.digits,
            symbols: r.symbols,
            unambiguous: r.unambiguous,
            avoid: String::new(),
        }
    }
}

impl Recipe {
    /// A passphrase of `words` words with the defaults.
    pub fn words(words: usize) -> Self {
        let d = PassphraseRule::default();
        Self::Words {
            words,
            separator: d.separator,
            capitalize: d.capitalize,
            number: d.number,
        }
    }

    fn character_rule(&self) -> Result<Option<CharacterRule>> {
        match self {
            Self::Characters {
                length,
                digits,
                symbols,
                unambiguous,
                avoid,
            } => PasswordRule {
                length: *length,
                letters: true,
                digits: *digits,
                symbols: *symbols,
                unambiguous: *unambiguous,
            }
            .to_character_rule()
            .without(avoid)
            .map(Some),
            Self::Words { .. } => Ok(None),
        }
    }

    fn passphrase_rule(&self) -> Option<PassphraseRule> {
        match self {
            Self::Words {
                words,
                separator,
                capitalize,
                number,
            } => Some(PassphraseRule {
                words: *words,
                separator: separator.clone(),
                capitalize: *capitalize,
                number: *number,
            }),
            Self::Characters { .. } => None,
        }
    }

    /// Checks the options without making anything.
    pub fn check(&self) -> Result<()> {
        match (self.character_rule()?, self.passphrase_rule()) {
            (Some(r), _) => r.check(MIN_LENGTH),
            (_, Some(p)) => p.check(),
            (None, None) => unreachable!("a recipe is one style or the other"),
        }
    }

    /// Makes one value. The text is wiped when dropped.
    pub fn generate(&self) -> Result<Zeroizing<String>> {
        match (self.character_rule()?, self.passphrase_rule()) {
            (Some(r), _) => r.generate(),
            (_, Some(p)) => p.generate(),
            (None, None) => unreachable!("a recipe is one style or the other"),
        }
    }

    /// The value under a site's own rules: a character password inside
    /// the site's bounds and sets, whatever style was asked (a site's
    /// rules are about characters, so a passphrase can't promise them).
    pub fn generate_for(&self, site: &SiteRule) -> Result<Zeroizing<String>> {
        site.generate(self.length_hint(), self.avoid())
    }

    /// The character rule a site's rules make of these options.
    pub fn site_rule(&self, site: &SiteRule) -> Result<CharacterRule> {
        site.character_rule(self.length_hint(), self.avoid())
    }

    fn length_hint(&self) -> usize {
        match self {
            Self::Characters { length, .. } => *length,
            Self::Words { .. } => PasswordRule::STRONG.length,
        }
    }

    fn avoid(&self) -> &str {
        match self {
            Self::Characters { avoid, .. } => avoid,
            Self::Words { .. } => "",
        }
    }

    /// The bits a value made this way carries.
    pub fn entropy_bits(&self) -> Result<f64> {
        Ok(match (self.character_rule()?, self.passphrase_rule()) {
            (Some(r), _) => r.entropy_bits(),
            (_, Some(p)) => p.entropy_bits(),
            (None, None) => unreachable!("a recipe is one style or the other"),
        })
    }

    /// The options in plain words.
    pub fn describe(&self) -> String {
        match self {
            Self::Characters {
                length,
                digits,
                symbols,
                unambiguous,
                avoid,
            } => {
                let mut s = PasswordRule {
                    length: *length,
                    letters: true,
                    digits: *digits,
                    symbols: *symbols,
                    unambiguous: *unambiguous,
                }
                .describe();
                if !avoid.is_empty() {
                    s.push_str(&format!(", without {avoid}"));
                }
                s
            }
            Self::Words { .. } => self
                .passphrase_rule()
                .map(|p| p.describe())
                .unwrap_or_default(),
        }
    }
}

/// One index below `n` (at most 256), uniform: a random byte is drawn
/// again whenever it lands in the uneven tail above the largest multiple
/// of `n`.
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

/// One index below `n` (any size up to `u32::MAX`), uniform, from four
/// random bytes with the same rejection of the uneven tail.
fn uniform_below_wide(n: usize) -> Result<usize> {
    let n = u64::try_from(n).map_err(|_| Error::Invalid("too many choices".into()))?;
    debug_assert!(n > 0 && n <= u64::from(u32::MAX));
    let space = 1u64 << 32;
    let limit = space - (space % n);
    loop {
        let mut b = [0u8; 4];
        random_bytes(&mut b)?;
        let v = u64::from(u32::from_le_bytes(b));
        if v < limit {
            return Ok((v % n) as usize);
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
    rule.to_character_rule().generate()
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
        let r = Recipe::Characters {
            length: 16,
            digits: true,
            symbols: false,
            unambiguous: false,
            avoid: "xyz".into(),
        };
        assert_eq!(
            r.describe(),
            "16 characters: letters and digits, without xyz"
        );
        let w = Recipe::Words {
            words: 5,
            separator: " ".into(),
            capitalize: true,
            number: true,
        };
        assert_eq!(
            w.describe(),
            "5 words joined by spaces, each with a capital, one with a digit"
        );
    }

    #[test]
    fn entropy_is_counted_from_the_alphabet_and_the_list() {
        let strong = PasswordRule::STRONG.entropy_bits();
        assert!((strong - 20.0 * 76f64.log2()).abs() < 1e-9, "{strong}");
        assert!(strong > 124.0 && strong < 126.0);
        let readable = PasswordRule::READABLE.entropy_bits();
        assert!(readable > 113.0 && readable < 114.0, "{readable}");
        assert_eq!(words().len(), 7776);
        assert!((bits_per_word() - 12.925).abs() < 0.001);
        let six = Recipe::words(6).entropy_bits().unwrap();
        assert!((six - 6.0 * 7776f64.log2()).abs() < 1e-9, "{six}");
        let with_number = Recipe::Words {
            words: 6,
            separator: "-".into(),
            capitalize: true,
            number: true,
        }
        .entropy_bits()
        .unwrap();
        assert!((with_number - six - 60f64.log2()).abs() < 1e-9);
    }

    #[test]
    fn the_word_list_is_the_eff_long_list() {
        let w = words();
        assert_eq!(w.first(), Some(&"abacus"));
        assert_eq!(w.last(), Some(&"zoom"));
        let mut sorted = w.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 7776, "no word twice");
        assert!(
            w.iter()
                .all(|x| x.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'))
        );
        assert!(w.iter().all(|x| (3..=9).contains(&x.len())));
    }

    #[test]
    fn a_passphrase_is_words_from_the_list_with_the_options() {
        let list = words();
        for _ in 0..100 {
            let p = Recipe::Words {
                words: 5,
                separator: " ".into(),
                capitalize: false,
                number: false,
            }
            .generate()
            .unwrap();
            let parts: Vec<&str> = p.split(' ').collect();
            assert_eq!(parts.len(), 5, "{}", p.as_str());
            assert!(parts.iter().all(|w| list.contains(w)));
            let dashed = Recipe::words(5).generate().unwrap();
            assert!(dashed.split('-').count() >= 5);
        }
        let r = Recipe::Words {
            words: 4,
            separator: ".".into(),
            capitalize: true,
            number: true,
        };
        for _ in 0..100 {
            let p = r.generate().unwrap();
            let words: Vec<&str> = p.split('.').collect();
            assert_eq!(words.len(), 4);
            assert!(words.iter().all(|w| w.as_bytes()[0].is_ascii_uppercase()));
            let digits = p.bytes().filter(u8::is_ascii_digit).count();
            assert_eq!(digits, 1, "{}", p.as_str());
            let with_digit = words
                .iter()
                .filter(|w| w.bytes().last().is_some_and(|b| b.is_ascii_digit()))
                .count();
            assert_eq!(with_digit, 1);
            for w in words {
                let bare = w
                    .trim_end_matches(|c: char| c.is_ascii_digit())
                    .to_ascii_lowercase();
                assert!(list.contains(&bare.as_str()), "{bare}");
            }
        }
        for bad in [
            Recipe::words(2),
            Recipe::words(MAX_WORDS + 1),
            Recipe::Words {
                words: 5,
                separator: "x".into(),
                capitalize: false,
                number: false,
            },
            Recipe::Words {
                words: 5,
                separator: "--".into(),
                capitalize: false,
                number: false,
            },
        ] {
            assert!(bad.generate().is_err(), "{bad:?}");
            assert!(bad.check().is_err());
        }
    }

    #[test]
    fn options_lower_to_a_rule_and_avoid_leaves_characters_out() {
        let r = Recipe::Characters {
            length: 32,
            digits: true,
            symbols: true,
            unambiguous: false,
            avoid: "!#$%&*+-=?@^_".into(),
        };
        for _ in 0..100 {
            let p = r.generate().unwrap();
            assert_eq!(p.len(), 32);
            // Only `~` is left of the symbols, so every password has it.
            assert!(p.contains('~'));
            assert!(!p.bytes().any(|b| b"!#$%&*+-=?@^_".contains(&b)));
        }
        let no_digits_left = Recipe::Characters {
            length: 20,
            digits: true,
            symbols: false,
            unambiguous: false,
            avoid: DIGITS.into(),
        };
        assert!(no_digits_left.generate().is_err());
        let short = Recipe::Characters {
            length: 6,
            digits: true,
            symbols: true,
            unambiguous: false,
            avoid: String::new(),
        };
        assert!(short.check().is_err());
        let json = serde_json_like(&r);
        assert!(json.contains("characters"));
    }

    fn serde_json_like(r: &Recipe) -> String {
        // The crate has no JSON dependency; CBOR round-trips the same shape.
        let bytes = crate::cbor::to_canonical(r).unwrap();
        let back: Recipe = crate::cbor::from_slice(&bytes).unwrap();
        assert_eq!(&back, r);
        format!("{r:?}").to_lowercase()
    }

    #[test]
    fn a_sites_rule_reads_and_bounds_what_is_made() {
        let rule = SiteRule::parse(
            "minlength: 8; maxlength: 12; required: lower; required: upper; required: digit; required: [!#$]; max-consecutive: 2;",
        )
        .unwrap();
        assert_eq!(rule.min_length, Some(8));
        assert_eq!(rule.max_length, Some(12));
        assert_eq!(rule.max_consecutive, Some(2));
        assert_eq!(rule.required.len(), 4);
        assert_eq!(rule.required[3], b"!#$".to_vec());
        for _ in 0..200 {
            let p = rule.generate(20, "").unwrap();
            assert_eq!(p.len(), 12, "moved down to the site's maximum");
            assert!(p.bytes().any(|b| b"!#$".contains(&b)));
            assert!(
                p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$".contains(&b))
            );
            let runs = p
                .as_bytes()
                .windows(3)
                .any(|w| w[0] == w[1] && w[1] == w[2]);
            assert!(!runs, "{}", p.as_str());
        }
        // Digits only (a bank's PIN-like password), and a floor below 8.
        let pin = SiteRule::parse("minlength: 6; maxlength: 6; required: digit;").unwrap();
        let p = pin.generate(20, "").unwrap();
        assert_eq!(p.len(), 6);
        assert!(p.bytes().all(|b| b.is_ascii_digit()));
        // A set holding `]` and `-`, and the text written back reads the same.
        let odd = SiteRule::parse("required: []-]; allowed: upper, lower;").unwrap();
        assert_eq!(odd.required[0], b"-]".to_vec());
        let tail = SiteRule::parse("required: [-!@]]; allowed: lower;").unwrap();
        assert_eq!(tail.required[0], b"!-@]".to_vec());
        assert_eq!(SiteRule::parse(&odd.to_text()).unwrap(), odd);
        assert_eq!(SiteRule::parse(&rule.to_text()).unwrap(), rule);
        // Nothing named: letters and digits at our length.
        let none = SiteRule::parse("minlength: 6;").unwrap();
        let p = none.generate(20, "").unwrap();
        assert_eq!(p.len(), 20);
        assert!(p.bytes().all(|b| b.is_ascii_alphanumeric()));
        // `special` draws from the symbols every form takes.
        let special = SiteRule::parse("required: special; allowed: lower;").unwrap();
        let p = special.generate(16, "").unwrap();
        assert!(p.bytes().any(|b| SYMBOLS.as_bytes().contains(&b)));
        assert!(!p.contains(' ') && !p.contains('"') && !p.contains('\\'));
        // Malformed rules are refused, unknown properties skipped.
        assert!(SiteRule::parse("minlength: eight;").is_err());
        assert!(SiteRule::parse("required: [abc").is_err());
        assert!(SiteRule::parse("required: emoji;").is_err());
        assert!(SiteRule::parse("minlength: 12; maxlength: 8;").is_err());
        assert!(SiteRule::parse("future-thing: 3; minlength: 9;").is_ok());
        assert!(
            SiteRule::parse("maxlength: 3;")
                .unwrap()
                .generate(20, "")
                .is_err()
        );
        assert_eq!(
            rule.describe(),
            "8 to 12 characters, with a lower-case letter, a capital, a digit and one of !#$, no character more than 2 times in a row"
        );
    }

    #[test]
    fn every_rule_in_the_published_snapshot_reads() {
        // The rules the app bundles (crates/app/data/password-rules.json),
        // read here line by line without a JSON parser: each must parse,
        // and nearly all must make a password.
        let data = include_str!("../../app/data/password-rules.json");
        let mut read = 0;
        for line in data.lines() {
            let Some(rest) = line.trim().strip_prefix("\"password-rules\": \"") else {
                continue;
            };
            let text = rest.trim_end_matches(',').trim_end_matches('"');
            let text = text.replace("\\\"", "\"").replace("\\\\", "\\");
            let rule = SiteRule::parse(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
            rule.generate(20, "")
                .unwrap_or_else(|e| panic!("{text}: {e}"));
            read += 1;
        }
        assert!(read > 400, "{read}");
    }

    /// Statistical checks of the generator's output (matrix
    /// `password-generator`: "statistical randomness tests in CI"). Each
    /// threshold is far in the tail for a fair source (odds well under one
    /// in a million of a false alarm) while a biased draw fails it: a
    /// modulo bias, a stuck position or a repeated word would.
    mod stats {
        use super::*;

        /// The chi-squared statistic of `counts` against a flat expectation.
        fn chi_squared(counts: &[f64]) -> f64 {
            let total: f64 = counts.iter().sum();
            let expected = total / counts.len() as f64;
            counts
                .iter()
                .map(|c| (c - expected).powi(2) / expected)
                .sum()
        }

        /// A chi-squared bound for `dof` degrees of freedom at about
        /// p = 1e-7: the Wilson-Hilferty approximation with z = 5.2.
        fn bound(dof: f64) -> f64 {
            let z = 5.2;
            let t = 1.0 - 2.0 / (9.0 * dof) + z * (2.0 / (9.0 * dof)).sqrt();
            dof * t.powi(3)
        }

        #[test]
        fn characters_are_uniform_over_the_alphabet() {
            let rule = PasswordRule {
                length: 100,
                ..PasswordRule::STRONG
            }
            .to_character_rule();
            let alphabet = rule.alphabet();
            let mut counts = vec![0.0; alphabet.len()];
            for _ in 0..760 {
                for b in rule.generate().unwrap().bytes() {
                    let i = alphabet.binary_search(&b).unwrap();
                    counts[i] += 1.0;
                }
            }
            let chi = chi_squared(&counts);
            assert!(
                chi < bound(75.0),
                "chi-squared {chi} over {} symbols",
                alphabet.len()
            );
        }

        #[test]
        fn every_position_is_uniform_over_the_classes() {
            // Position by position (a stuck or biased slot would show),
            // over the four classes. The every-class rule shifts each
            // position alike, so the positions are compared with each other.
            let rule = PasswordRule::STRONG.to_character_rule();
            let class_of = |b: u8| -> usize {
                if b.is_ascii_lowercase() {
                    0
                } else if b.is_ascii_uppercase() {
                    1
                } else if b.is_ascii_digit() {
                    2
                } else {
                    3
                }
            };
            let draws = 4000;
            let mut per_pos = vec![[0.0f64; 4]; rule.length];
            for _ in 0..draws {
                for (i, b) in rule.generate().unwrap().bytes().enumerate() {
                    per_pos[i][class_of(b)] += 1.0;
                }
            }
            // And positions agree with each other (homogeneity).
            let mut pooled = [0.0f64; 4];
            for c in &per_pos {
                for k in 0..4 {
                    pooled[k] += c[k];
                }
            }
            let mut chi = 0.0;
            for c in &per_pos {
                for k in 0..4 {
                    let expected = pooled[k] / rule.length as f64;
                    chi += (c[k] - expected).powi(2) / expected;
                }
            }
            let dof = ((rule.length - 1) * 3) as f64;
            assert!(
                chi < bound(dof),
                "positions differ: chi-squared {chi} on {dof}"
            );
        }

        #[test]
        fn neighbouring_characters_are_independent() {
            // Serial test over pairs of classes: a generator whose next
            // character leans on the last one fails it.
            let rule = PasswordRule {
                length: 100,
                ..PasswordRule::READABLE
            }
            .to_character_rule();
            let class_of =
                |b: u8| usize::from(b.is_ascii_uppercase()) + 2 * usize::from(b.is_ascii_digit());
            let mut pairs = [[0.0f64; 3]; 3];
            let mut singles = [0.0f64; 3];
            for _ in 0..400 {
                let p = rule.generate().unwrap();
                let bytes = p.as_bytes();
                for w in bytes.windows(2) {
                    pairs[class_of(w[0])][class_of(w[1])] += 1.0;
                }
                for b in bytes {
                    singles[class_of(*b)] += 1.0;
                }
            }
            let n: f64 = singles.iter().sum();
            let m: f64 = pairs.iter().flatten().sum();
            let mut chi = 0.0;
            for a in 0..3 {
                for b in 0..3 {
                    let expected = m * (singles[a] / n) * (singles[b] / n);
                    chi += (pairs[a][b] - expected).powi(2) / expected;
                }
            }
            assert!(chi < bound(4.0), "pairs lean: chi-squared {chi}");
        }

        #[test]
        fn passphrase_words_are_uniform_over_the_list() {
            // 7,776 bins, 8 words each expected on average: grouped into
            // 243 bins of 32 consecutive words so each holds about 256.
            let list = words();
            let mut index = std::collections::BTreeMap::new();
            for (i, w) in list.iter().enumerate() {
                index.insert(*w, i);
            }
            let rule = PassphraseRule {
                words: 20,
                separator: " ".into(),
                capitalize: false,
                number: false,
            };
            let mut counts = vec![0.0f64; 243];
            for _ in 0..3110 {
                for w in rule.generate().unwrap().split(' ') {
                    counts[index[w] / 32] += 1.0;
                }
            }
            let chi = chi_squared(&counts);
            assert!(chi < bound(242.0), "chi-squared {chi} over word groups");
            // The digit is uniform too.
            let numbered = PassphraseRule {
                words: 3,
                separator: " ".into(),
                capitalize: false,
                number: true,
            };
            let mut digits = [0.0f64; 10];
            for _ in 0..5000 {
                let p = numbered.generate().unwrap();
                let d = p.bytes().find(u8::is_ascii_digit).unwrap();
                digits[usize::from(d - b'0')] += 1.0;
            }
            let chi = chi_squared(&digits);
            assert!(chi < bound(9.0), "digits: chi-squared {chi}");
        }

        #[test]
        fn the_source_passes_monobit_and_runs() {
            // NIST SP 800-22 §2.1 (frequency) and §2.3 (runs) on a megabit
            // of the operating system's output.
            let mut bytes = vec![0u8; 1 << 17];
            random_bytes(&mut bytes).unwrap();
            let n = (bytes.len() * 8) as f64;
            let ones: f64 = bytes.iter().map(|b| f64::from(b.count_ones())).sum();
            let s_obs = (2.0 * ones - n).abs() / n.sqrt();
            assert!(s_obs < 5.5, "monobit statistic {s_obs}");
            let pi = ones / n;
            let mut runs = 1.0;
            let mut last = bytes[0] & 1;
            for b in &bytes {
                for k in 0..8 {
                    let bit = (b >> k) & 1;
                    if bit != last {
                        runs += 1.0;
                        last = bit;
                    }
                }
            }
            let expected = 2.0 * n * pi * (1.0 - pi);
            let z = (runs - expected).abs() / (2.0 * (2.0 * n).sqrt() * pi * (1.0 - pi));
            assert!(z < 5.5, "runs statistic {z}");
        }

        #[test]
        fn the_wide_draw_has_no_modulo_bias() {
            // 7,776 does not divide 2^32; the rejection keeps it flat. Bins
            // of 1,296 (7,776 / 6) catch a lean toward the low indices.
            let mut counts = [0.0f64; 6];
            for _ in 0..60_000 {
                counts[uniform_below_wide(7776).unwrap() / 1296] += 1.0;
            }
            let chi = chi_squared(&counts);
            assert!(chi < bound(5.0), "chi-squared {chi}");
        }
    }
}
