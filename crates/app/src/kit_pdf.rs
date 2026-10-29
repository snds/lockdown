//! The recovery kit as a PDF file (docs/plan/38-mac-fixes.md §4).
//!
//! The kit's page is made once (`render_kit` in `session.rs`), shown once,
//! and never kept. A person who wants it as a file asks while the page is
//! still in front of them: the shell hands back the page it was shown, and
//! this module lays it out as one page the way the dialog prints it: the
//! title, the notes, the Secret Key and the recovery code in large type in
//! groups of four (the characters a person types, without the dashes), the
//! account lines and the date, the page's own advice, and where to keep the
//! file. The bytes are the same on the desktop (written by the shell to the
//! place the person picked) and in the web vault (handed to the page as a
//! download).
//!
//! **The paper** is US Letter where the computer's region uses it (the
//! United States, Canada, Mexico, the Philippines and the other Letter
//! regions of CLDR's paper-size data) and A4 everywhere else ([`Paper`];
//! D459 as changed under D375). The region comes from the OS locale.
//!
//! **The bytes are secret.** Everything that holds the kit is wiped when
//! it drops: the page's text, the encoded strings, the content stream and
//! the finished file. The writer is given room for the whole file up front,
//! so it does not grow by copying (a grown buffer leaves the old one in
//! freed memory, unwiped). Nothing here is logged.
//!
//! **The font** is JetBrains Mono (SIL OFL 1.1), cut down to the
//! Windows-1252 characters and embedded, because the kit must not confuse
//! `0` with `O` or `1` with `l` and `I`: its zero is dotted, its `l` has a
//! tail, its `I` has bars. The text is encoded as WinAnsi, so a character
//! outside that set (rare in a login name or a device name) prints as `?`.

use std::fmt::Write as _;

use pdf_writer::types::FontFlags;
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use zeroize::Zeroizing;

/// The name the save dialog suggests.
pub const FILE_NAME: &str = "Lockdown Recovery Kit.pdf";

/// What the file says about itself, under "Where to keep this file".
pub const KEEP_WORDS: &str = "This file holds everything needed to get into your account with your password. Keep it off this computer: on a USB drive you put away, as a printed copy, or in a safe. Lockdown keeps no copy.";

/// The kit page's first line, as `render_kit` writes it.
const HEADER: &str = "LOCKDOWN RECOVERY KIT";

/// The page is a few hundred bytes; anything far bigger is not a kit page.
const MAX_PAGE: usize = 8 * 1024;
/// The notes the dialog shows above the page: a handful of short lines.
const MAX_NOTES: usize = 8;
const MAX_NOTE: usize = 400;

/// The fields `render_kit` writes, in its order.
const LABELS: [&str; 6] = [
    "Secret Key",
    "Recovery code",
    "Login name",
    "Server",
    "Profile",
    "Made",
];

static REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular-WinAnsi.ttf");
static BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold-WinAnsi.ttf");

/// Both cuts: 1000 units to the em, every glyph 600 wide (monospaced),
/// ascender 1020, descender -300, capitals 730 tall (the font's own
/// `head`, `hhea` and `OS/2`, checked by a test).
const ADVANCE: f32 = 0.6;
const UNITS: f32 = 1000.0;

const MARGIN: f32 = 56.0;

/// The regions that print on US Letter (CLDR's `paperSize` data: every
/// other region uses A4).
const LETTER_REGIONS: [&str; 14] = [
    "BZ", "CA", "CL", "CO", "CR", "GT", "MX", "NI", "PA", "PH", "PR", "SV", "US", "VE",
];

/// The paper the kit is laid out on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paper {
    /// 210 × 297 mm.
    A4,
    /// 8.5 × 11 in.
    Letter,
}

impl Paper {
    /// Width and height in points.
    pub fn size(self) -> (f32, f32) {
        match self {
            Self::A4 => (595.28, 841.89),
            Self::Letter => (612.0, 792.0),
        }
    }

    /// The paper a region prints on, from its two-letter code.
    pub fn for_region(region: &str) -> Self {
        let region = region.trim().to_ascii_uppercase();
        if LETTER_REGIONS.contains(&region.as_str()) {
            Self::Letter
        } else {
            Self::A4
        }
    }

    /// The paper of a locale tag (`en_US.UTF-8`, `en-US`, `fr_CA@euro`,
    /// `zh-Hant-TW`): its region subtag, or A4 when it names none.
    pub fn for_locale(tag: &str) -> Self {
        let tag = tag.split(['.', '@']).next().unwrap_or_default();
        tag.split(['_', '-'])
            .skip(1)
            .find(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
            .map_or(Self::A4, Self::for_region)
    }

    /// The paper of this computer's region (the OS locale).
    pub fn for_system() -> Self {
        system_locale().map_or(Self::A4, |tag| Self::for_locale(&tag))
    }
}

/// The OS locale's tag, where this build can read it: the paper setting
/// or the language variables first (`LC_ALL`, `LC_PAPER`, `LANG`), then on
/// a Mac the system's own locale (`AppleLocale`), which an app started
/// from the Dock sees no variable for.
fn system_locale() -> Option<String> {
    for var in ["LC_ALL", "LC_PAPER", "LANG"] {
        if let Ok(v) = std::env::var(var)
            && !v.is_empty()
            && v != "C"
            && v != "POSIX"
            && !v.starts_with("C.")
        {
            return Some(v);
        }
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/bin/defaults")
            .args(["read", "-g", "AppleLocale"])
            .output()
            .ok()?;
        let tag = String::from_utf8(out.stdout).ok()?.trim().to_string();
        if out.status.success() && !tag.is_empty() {
            return Some(tag);
        }
    }
    None
}

/// The large type: the size, the extra room between characters, and the
/// gap between groups of four.
const BIG: f32 = 26.0;
const BIG_SPACING: f32 = 4.0;
const GROUP_GAP: f32 = 22.0;
const GROUPS_PER_ROW: usize = 4;
const GROUP: usize = 4;

/// One piece of the kit page, in the page's order.
#[derive(Debug)]
enum Block<'a> {
    /// A paragraph of the page's own words, unwrapped.
    Prose(Vec<&'a str>),
    /// A labelled line (`Secret Key:  …`).
    Field(&'a str, &'a str),
}

/// The kit page, read: every value borrowed from the caller's text, so no
/// copy of a secret is made here.
#[derive(Debug)]
pub struct KitPage<'a> {
    blocks: Vec<Block<'a>>,
}

impl<'a> KitPage<'a> {
    /// Reads a page `render_kit` wrote. Anything else is refused: this is
    /// not a way to make any PDF the caller likes.
    pub fn parse(page: &'a str) -> Result<Self, String> {
        if page.len() > MAX_PAGE {
            return Err("that isn't a recovery kit page".into());
        }
        let mut lines = page.lines().map(str::trim_end);
        if lines.by_ref().find(|l| !l.trim().is_empty()) != Some(HEADER) {
            return Err("that isn't a recovery kit page".into());
        }
        let mut blocks: Vec<Block<'a>> = Vec::new();
        let mut prose: Vec<&'a str> = Vec::new();
        let flush = |prose: &mut Vec<&'a str>, blocks: &mut Vec<Block<'a>>| {
            if !prose.is_empty() {
                blocks.push(Block::Prose(std::mem::take(prose)));
            }
        };
        for line in lines {
            if line.trim().is_empty() || line.chars().all(|c| c == '=') {
                flush(&mut prose, &mut blocks);
                continue;
            }
            match field(line) {
                Some((label, value)) => {
                    flush(&mut prose, &mut blocks);
                    if blocks
                        .iter()
                        .any(|b| matches!(b, Block::Field(l, _) if *l == label))
                    {
                        return Err(format!("the kit page names its {label} twice"));
                    }
                    blocks.push(Block::Field(label, value));
                }
                None => prose.push(line.trim()),
            }
        }
        flush(&mut prose, &mut blocks);
        let kit = Self { blocks };
        for label in ["Secret Key", "Recovery code"] {
            let value = kit
                .value(label)
                .ok_or_else(|| format!("the kit page has no {label}"))?;
            if value.is_empty() || !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(format!("the kit page's {label} isn't readable"));
            }
        }
        Ok(kit)
    }

    fn value(&self, label: &str) -> Option<&'a str> {
        self.blocks.iter().find_map(|b| match b {
            Block::Field(l, v) if *l == label => Some(*v),
            _ => None,
        })
    }

    /// The Secret Key as a person types it, in groups of four.
    fn groups(&self, label: &str) -> Vec<Zeroizing<String>> {
        let typed: Zeroizing<String> = Zeroizing::new(
            self.value(label)
                .unwrap_or_default()
                .chars()
                .filter(|c| *c != '-')
                .collect(),
        );
        typed
            .as_bytes()
            .chunks(GROUP)
            .map(|c| Zeroizing::new(String::from_utf8_lossy(c).into_owned()))
            .collect()
    }
}

/// `Label:   value` for one of the kit's labels.
fn field(line: &str) -> Option<(&'static str, &str)> {
    let (label, value) = line.split_once(':')?;
    let label = LABELS.iter().find(|l| **l == label.trim())?;
    let value = value.trim();
    (!value.is_empty()).then_some((*label, value))
}

/// Checks that `page` is a kit page, without laying it out (the shell asks
/// before it opens a save dialog, so a refusal comes first).
pub fn check(page: &str) -> Result<(), String> {
    KitPage::parse(page).map(|_| ())
}

/// The kit page as a one-page PDF. `notes` are the lines the dialog shows
/// above the page (what changed when the kit was made).
pub fn render(notes: &[String], page: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    render_on(Paper::for_system(), notes, page)
}

/// [`render`] on the paper given.
pub fn render_on(paper: Paper, notes: &[String], page: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    if notes.len() > MAX_NOTES || notes.iter().any(|n| n.len() > MAX_NOTE) {
        return Err("those notes don't belong on a recovery kit".into());
    }
    let kit = KitPage::parse(page)?;
    let content = layout(paper, notes, &kit);
    Ok(assemble(paper, &content))
}

/// A run of text on the page: which face, how big, and where its baseline starts.
struct Sheet {
    content: Content,
    y: f32,
    /// The page's width.
    w: f32,
}

const REG: Name<'static> = Name(b"F1");
const BLD: Name<'static> = Name(b"F2");

impl Sheet {
    fn text(&mut self, font: Name<'_>, size: f32, x: f32, gray: f32, text: &str) {
        let bytes = win_ansi(text);
        self.content
            .set_fill_gray(gray)
            .begin_text()
            .set_font(font, size)
            .next_line(x, self.y)
            .show(Str(&bytes))
            .end_text();
    }

    /// A paragraph, wrapped to the page, `size` points, one line every `lead`.
    fn paragraph(&mut self, font: Name<'_>, size: f32, gray: f32, text: &str) {
        let lead = size * 1.45;
        let cols = ((self.w - 2.0 * MARGIN) / (size * ADVANCE)).floor() as usize;
        for line in wrap(text, cols) {
            self.y -= lead;
            self.text(font, size, MARGIN, gray, &line);
        }
    }

    fn rule(&mut self) {
        self.content
            .set_stroke_gray(0.7)
            .set_line_width(0.75)
            .move_to(MARGIN, self.y)
            .line_to(self.w - MARGIN, self.y)
            .stroke();
    }

    /// A heading with a rule under it, as LargeTypeView draws its title.
    fn heading(&mut self, text: &str, right: Option<&str>) {
        self.y -= 30.0;
        self.text(BLD, 12.0, MARGIN, 0.0, text);
        if let Some(right) = right {
            let w = right.chars().count() as f32 * 9.0 * ADVANCE;
            self.text(REG, 9.0, self.w - MARGIN - w, 0.35, right);
        }
        self.y -= 8.0;
        self.rule();
    }

    /// A value in large type, groups of four, four groups to a row.
    fn large(&mut self, groups: &[Zeroizing<String>]) {
        let group_w = GROUP as f32 * (BIG * ADVANCE + BIG_SPACING);
        for row in groups.chunks(GROUPS_PER_ROW) {
            self.y -= BIG * 1.5;
            let mut x = MARGIN;
            for g in row {
                let bytes = win_ansi(g);
                self.content
                    .set_fill_gray(0.0)
                    .begin_text()
                    .set_font(REG, BIG)
                    .set_char_spacing(BIG_SPACING)
                    .next_line(x, self.y)
                    .show(Str(&bytes))
                    .set_char_spacing(0.0)
                    .end_text();
                x += group_w + GROUP_GAP;
            }
        }
        self.y -= 6.0;
    }
}

fn layout(paper: Paper, notes: &[String], kit: &KitPage<'_>) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(lay_out(paper, notes, kit).content.finish().into_vec())
}

fn lay_out(paper: Paper, notes: &[String], kit: &KitPage<'_>) -> Sheet {
    let (w, h) = paper.size();
    let mut sheet = Sheet {
        // The page's content is a few kilobytes; room for all of it at once.
        content: Content::with_capacity(32 * 1024),
        y: h - MARGIN,
        w,
    };
    let made = kit.value("Made").unwrap_or_default();
    let date = made.split_whitespace().next().unwrap_or_default();

    sheet.y -= 22.0;
    sheet.text(BLD, 22.0, MARGIN, 0.0, "Lockdown recovery kit");
    if !made.is_empty() {
        sheet.y -= 18.0;
        sheet.text(REG, 10.0, MARGIN, 0.35, &format!("Made {made}"));
    }
    if !notes.is_empty() {
        sheet.y -= 6.0;
        for note in notes {
            sheet.paragraph(REG, 10.0, 0.25, note);
        }
    }

    for label in ["Secret Key", "Recovery code"] {
        sheet.heading(label, (!date.is_empty()).then_some(date));
        sheet.large(&kit.groups(label));
    }

    // The account lines, in the page's order, the label in its own column.
    sheet.y -= 14.0;
    for block in &kit.blocks {
        if let Block::Field(label, value) = block
            && !matches!(*label, "Secret Key" | "Recovery code")
        {
            sheet.y -= 15.0;
            sheet.text(BLD, 10.0, MARGIN, 0.0, &format!("{label}:"));
            sheet.text(REG, 10.0, MARGIN + 96.0, 0.0, value);
        }
    }

    // The page's own words: where to keep it and how it is used.
    sheet.y -= 8.0;
    for block in &kit.blocks {
        if let Block::Prose(lines) = block {
            sheet.y -= 6.0;
            sheet.paragraph(REG, 10.0, 0.0, &lines.join(" "));
        }
    }

    sheet.heading("Where to keep this file", None);
    sheet.y -= 4.0;
    sheet.paragraph(REG, 10.0, 0.0, KEEP_WORDS);
    sheet
}

/// The file around the content: one page, the two embedded faces.
fn assemble(paper: Paper, content: &[u8]) -> Zeroizing<Vec<u8>> {
    let (page_w, page_h) = paper.size();
    let catalog = Ref::new(1);
    let pages = Ref::new(2);
    let page = Ref::new(3);
    let contents = Ref::new(4);
    let info = Ref::new(5);
    let faces = [
        (
            REG,
            b"JetBrainsMono-Regular".as_slice(),
            REGULAR,
            Ref::new(6),
            400.0,
        ),
        (
            BLD,
            b"JetBrainsMono-Bold".as_slice(),
            BOLD,
            Ref::new(9),
            700.0,
        ),
    ];

    // Room for everything up front, so the buffer never grows by copying.
    let mut pdf = Pdf::with_capacity(REGULAR.len() + BOLD.len() + content.len() + 16 * 1024);
    pdf.catalog(catalog).pages(pages);
    pdf.pages(pages).kids([page]).count(1);
    {
        let mut p = pdf.page(page);
        p.media_box(Rect::new(0.0, 0.0, page_w, page_h))
            .parent(pages)
            .contents(contents);
        let mut resources = p.resources();
        let mut fonts = resources.fonts();
        for (name, _, _, id, _) in &faces {
            fonts.pair(*name, *id);
        }
    }
    pdf.stream(contents, content);
    pdf.document_info(info)
        .title(TextStr("Lockdown Recovery Kit"))
        .producer(TextStr("Lockdown"));

    for (_, base, file, id, weight) in faces {
        let descriptor = Ref::new(id.get() + 1);
        let program = Ref::new(id.get() + 2);
        // A simple TrueType font with the WinAnsi encoding: the viewer maps
        // each byte to its Unicode character and the font's own table.
        let mut font = pdf.indirect(id).dict();
        font.pair(Name(b"Type"), Name(b"Font"));
        font.pair(Name(b"Subtype"), Name(b"TrueType"));
        font.pair(Name(b"BaseFont"), Name(base));
        font.pair(Name(b"FirstChar"), 32);
        font.pair(Name(b"LastChar"), 255);
        font.insert(Name(b"Widths"))
            .array()
            .items((32..=255).map(|_| ADVANCE * UNITS));
        font.pair(Name(b"FontDescriptor"), descriptor);
        font.pair(Name(b"Encoding"), Name(b"WinAnsiEncoding"));
        font.finish();

        let mut d = pdf.font_descriptor(descriptor);
        d.name(Name(base))
            .flags(FontFlags::FIXED_PITCH | FontFlags::NON_SYMBOLIC)
            .bbox(Rect::new(-1735.0, -400.0, 765.0, 1120.0))
            .italic_angle(0.0)
            .ascent(1020.0)
            .descent(-300.0)
            .cap_height(730.0)
            .stem_v(if weight > 500.0 { 140.0 } else { 80.0 })
            .font_file2(program);
        d.finish();

        pdf.stream(program, file)
            .pair(Name(b"Length1"), file.len() as i32);
    }
    Zeroizing::new(pdf.finish())
}

/// `text` in WinAnsi (Windows-1252), one byte a character; `?` for the rest.
fn win_ansi(text: &str) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(text.chars().map(win_ansi_byte).collect())
}

fn win_ansi_byte(c: char) -> u8 {
    const HIGH: [(char, u8); 27] = [
        ('€', 0x80),
        ('‚', 0x82),
        ('ƒ', 0x83),
        ('„', 0x84),
        ('…', 0x85),
        ('†', 0x86),
        ('‡', 0x87),
        ('ˆ', 0x88),
        ('‰', 0x89),
        ('Š', 0x8A),
        ('‹', 0x8B),
        ('Œ', 0x8C),
        ('Ž', 0x8E),
        ('‘', 0x91),
        ('’', 0x92),
        ('“', 0x93),
        ('”', 0x94),
        ('•', 0x95),
        ('–', 0x96),
        ('—', 0x97),
        ('˜', 0x98),
        ('™', 0x99),
        ('š', 0x9A),
        ('›', 0x9B),
        ('œ', 0x9C),
        ('ž', 0x9E),
        ('Ÿ', 0x9F),
    ];
    match c as u32 {
        0x20..=0x7E | 0xA0..=0xFF => c as u8,
        _ => HIGH.iter().find(|(h, _)| *h == c).map_or(b'?', |(_, b)| *b),
    }
}

/// Greedy word wrap to `cols` characters; a word longer than a line is cut.
fn wrap(text: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(8);
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word;
        while word.chars().count() > cols {
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            let cut = word.char_indices().nth(cols).map_or(word.len(), |(i, _)| i);
            out.push(word[..cut].to_string());
            word = &word[cut..];
        }
        let need = line.chars().count() + usize::from(!line.is_empty()) + word.chars().count();
        if need > cols && !line.is_empty() {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        let _ = write!(line, "{word}");
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

/// Writes the file to `path` and nowhere else: no temporary file, no copy.
/// Readable by its owner only where the system has such modes. A failed
/// write takes the part-written file away again.
#[cfg(feature = "native")]
pub fn write_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let written = options
        .open(path)
        .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()));
    written.map_err(|e| {
        let _ = std::fs::remove_file(path);
        format!("the kit couldn't be saved there: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page in `render_kit`'s exact shape, with made-up values.
    pub(crate) const PAGE: &str = "\
LOCKDOWN RECOVERY KIT
=====================
Keep this page somewhere safe, away from your devices. You need it only
when every device is gone, or a brand-new device has no old device nearby.

Secret Key:     A3-K7QF2-M9XWD-4RHT6-PLC8Z-B3NVQ5
Recovery code:  RC4F-9K2M-XQ7H-TDPL-8B3Z-NW5V-6YEA-J2KD

Login name:     sam@rivera.example
Server:         https://vault.rivera.example
Profile:        3f2a9c1e5b7d4f608a1c2e3b4d5f6a7b
Made:           2026-10-29 on Sam’s MacBook

To get back in on a new device, choose \"Recover with the kit\" and give
your password, the Secret Key and the recovery code.
Lockdown never asks for this page in an email, a message or a web form.
";

    const SK_TYPED: &str = "A3K7QF2M9XWD4RHT6PLC8ZB3NVQ5";
    const RC_TYPED: &str = "RC4F9K2MXQ7HTDPL8B3ZNW5V6YEAJ2KD";

    /// A small reader for what this module writes: the header, the
    /// cross-reference table (every offset lands on its object), the
    /// trailer's catalog, the page tree to the one page, and the text its
    /// content stream shows, decoded from WinAnsi. Written for this test,
    /// so the parse does not trust the writer's own bookkeeping.
    pub(crate) struct Parsed {
        pub shown: Vec<String>,
        pub fonts: Vec<String>,
    }

    fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| p + from)
    }

    fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).rposition(|w| w == needle)
    }

    fn int_after(hay: &[u8], key: &[u8]) -> Option<usize> {
        let at = find(hay, key, 0)? + key.len();
        let digits: String = hay[at..]
            .iter()
            .skip_while(|b| b.is_ascii_whitespace())
            .take_while(|b| b.is_ascii_digit())
            .map(|b| *b as char)
            .collect();
        digits.parse().ok()
    }

    pub(crate) fn parse(pdf: &[u8]) -> Result<Parsed, String> {
        if !pdf.starts_with(b"%PDF-1.") {
            return Err("no header".into());
        }
        let tail = &pdf[pdf.len().saturating_sub(64)..];
        if find(tail, b"%%EOF", 0).is_none() {
            return Err("no %%EOF".into());
        }
        let sx = rfind(pdf, b"startxref").ok_or("no startxref")?;
        let xref_at = int_after(&pdf[sx..], b"startxref").ok_or("bad startxref")?;
        if !pdf[xref_at..].starts_with(b"xref") {
            return Err("startxref doesn't point at the table".into());
        }
        let table = std::str::from_utf8(&pdf[xref_at..sx]).map_err(|_| "xref not text")?;
        let mut rows = table.lines().skip(1);
        let head = rows.next().ok_or("no subsection")?;
        let count: usize = head
            .split_whitespace()
            .nth(1)
            .and_then(|n| n.parse().ok())
            .ok_or("bad subsection")?;
        let mut offsets = vec![0usize; count];
        for (n, row) in rows.take(count).enumerate() {
            let mut parts = row.split_whitespace();
            let off: usize = parts.next().and_then(|o| o.parse().ok()).ok_or("bad row")?;
            if parts.nth(1) == Some("n") {
                let want = format!("{n} 0 obj");
                if !pdf[off..].starts_with(want.as_bytes()) {
                    return Err(format!("object {n}'s offset is wrong"));
                }
                offsets[n] = off;
            }
        }
        let object = |n: usize| -> Result<&[u8], String> {
            let off = *offsets.get(n).filter(|o| **o > 0).ok_or("no such object")?;
            let end = find(pdf, b"endobj", off).ok_or("unterminated object")?;
            Ok(&pdf[off..end])
        };
        let trailer = &pdf[find(pdf, b"trailer", xref_at).ok_or("no trailer")?..sx];
        let root = int_after(trailer, b"/Root").ok_or("no root")?;
        let pages = int_after(object(root)?, b"/Pages").ok_or("no page tree")?;
        let tree = object(pages)?;
        if int_after(tree, b"/Count") != Some(1) {
            return Err("not one page".into());
        }
        let page_n = int_after(tree, b"/Kids [").ok_or("no kids")?;
        let page = object(page_n)?;
        let contents = int_after(page, b"/Contents").ok_or("no contents")?;
        let fonts = ["/F1", "/F2"]
            .iter()
            .map(|f| -> Result<String, String> {
                let id = int_after(page, f.as_bytes()).ok_or("no font")?;
                let font = object(id)?;
                let at = find(font, b"/BaseFont /", 0).ok_or("no base font")? + 11;
                let name: String = font[at..]
                    .iter()
                    .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
                    .map(|b| *b as char)
                    .collect();
                let desc = object(int_after(font, b"/FontDescriptor").ok_or("no descriptor")?)?;
                let prog = object(int_after(desc, b"/FontFile2").ok_or("not embedded")?)?;
                let len = int_after(prog, b"/Length").ok_or("no length")?;
                let start = find(prog, b"stream\n", 0).ok_or("no stream")? + 7;
                if prog.len() < start + len {
                    return Err("font stream shorter than its length".into());
                }
                Ok(name)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let stream = object(contents)?;
        let len = int_after(stream, b"/Length").ok_or("no length")?;
        let start = find(stream, b"stream\n", 0).ok_or("no stream")? + 7;
        let body = stream.get(start..start + len).ok_or("short stream")?;
        if !stream[start + len..]
            .trim_ascii_start()
            .starts_with(b"endstream")
        {
            return Err("stream length is wrong".into());
        }
        Ok(Parsed {
            shown: shown_strings(body)?,
            fonts,
        })
    }

    /// Every `(…) Tj` and `<…> Tj` in a content stream, unescaped and decoded.
    fn shown_strings(body: &[u8]) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < body.len() {
            if body[i] == b'<' && body.get(i + 1) != Some(&b'<') {
                let end = find(body, b">", i).ok_or("unterminated hex string")?;
                let hex: Vec<u8> = body[i + 1..end]
                    .iter()
                    .copied()
                    .filter(|b| !b.is_ascii_whitespace())
                    .collect();
                let bytes = hex
                    .chunks(2)
                    .map(|p| {
                        u8::from_str_radix(std::str::from_utf8(p).unwrap_or("?"), 16)
                            .map_err(|_| "bad hex string".to_string())
                    })
                    .collect::<Result<Vec<u8>, String>>()?;
                i = end + 1;
                if body[i..].trim_ascii_start().starts_with(b"Tj") {
                    out.push(bytes.iter().map(|b| decode_win_ansi(*b)).collect());
                }
                continue;
            }
            if body[i] != b'(' {
                i += 1;
                continue;
            }
            let mut s = Vec::new();
            let mut depth = 1;
            i += 1;
            while depth > 0 {
                let b = *body.get(i).ok_or("unterminated string")?;
                match b {
                    b'\\' => {
                        i += 1;
                        let e = *body.get(i).ok_or("bad escape")?;
                        s.push(match e {
                            b'n' => b'\n',
                            b'r' => b'\r',
                            b't' => b'\t',
                            b'0'..=b'7' => {
                                let mut v = u32::from(e - b'0');
                                for _ in 0..2 {
                                    match body.get(i + 1) {
                                        Some(d @ b'0'..=b'7') => {
                                            v = v * 8 + u32::from(d - b'0');
                                            i += 1;
                                        }
                                        _ => break,
                                    }
                                }
                                v as u8
                            }
                            other => other,
                        });
                    }
                    b'(' => {
                        depth += 1;
                        s.push(b);
                    }
                    b')' => {
                        depth -= 1;
                        if depth > 0 {
                            s.push(b);
                        }
                    }
                    _ => s.push(b),
                }
                i += 1;
            }
            let rest = body[i..].trim_ascii_start();
            if rest.starts_with(b"Tj") {
                out.push(s.iter().map(|b| decode_win_ansi(*b)).collect());
            }
        }
        Ok(out)
    }

    fn decode_win_ansi(b: u8) -> char {
        if b == 0x92 { '’' } else { b as char }
    }

    fn render_page() -> Zeroizing<Vec<u8>> {
        render(
            &["the account's slot now needs this Secret Key beside the password".into()],
            PAGE,
        )
        .unwrap()
    }

    #[test]
    fn letter_regions_get_letter_and_the_rest_a4() {
        for tag in [
            "en_US.UTF-8",
            "en-US",
            "fr_CA@euro",
            "es-MX",
            "en_PH",
            "es-CL",
            "es_PR.UTF-8",
        ] {
            assert_eq!(Paper::for_locale(tag), Paper::Letter, "{tag}");
        }
        for tag in [
            "en_GB.UTF-8",
            "de-DE",
            "fr_FR@euro",
            "ja_JP",
            "en",
            "",
            "zh-Hant-TW",
            "en-AU",
        ] {
            assert_eq!(Paper::for_locale(tag), Paper::A4, "{tag}");
        }
        assert_eq!(Paper::for_locale("zh-Hans-US"), Paper::Letter);
        assert_eq!(Paper::for_region("us"), Paper::Letter);
    }

    fn media_box(pdf: &[u8]) -> String {
        let raw = String::from_utf8_lossy(pdf);
        let at = raw.find("/MediaBox [").unwrap();
        raw[at..at + raw[at..].find(']').unwrap() + 1].to_string()
    }

    #[test]
    fn a_letter_kit_is_one_letter_page_and_everything_fits() {
        let pdf = render_on(Paper::Letter, &["a note".into()], PAGE).unwrap();
        assert_eq!(media_box(&pdf), "/MediaBox [0 0 612 792]");
        let parsed = parse(&pdf).unwrap();
        assert!(parsed.shown.iter().any(|s| s == "Lockdown recovery kit"));
        let kit = KitPage::parse(PAGE).unwrap();
        // The most notes a kit is made with today: three lines.
        let notes = vec![
            "the account's slot now needs this Secret Key beside the password; other devices learn it at their next sync".to_string();
            3
        ];
        assert!(lay_out(Paper::Letter, &notes, &kit).y >= MARGIN);
    }

    #[test]
    fn an_a4_kit_is_one_a4_page_and_everything_fits() {
        let pdf = render_on(Paper::A4, &["a note".into()], PAGE).unwrap();
        assert_eq!(media_box(&pdf), "/MediaBox [0 0 595.28 841.89]");
        let kit = KitPage::parse(PAGE).unwrap();
        // The most notes a kit is made with today: three lines.
        let notes = vec![
            "the account's slot now needs this Secret Key beside the password; other devices learn it at their next sync".to_string();
            3
        ];
        assert!(lay_out(Paper::A4, &notes, &kit).y >= MARGIN);
    }

    #[test]
    fn the_pdf_parses_with_both_faces_embedded() {
        let pdf = render_page();
        let parsed = parse(&pdf).unwrap();
        assert_eq!(
            parsed.fonts,
            ["JetBrainsMono-Regular", "JetBrainsMono-Bold"]
        );
        assert!(parsed.shown.iter().any(|s| s == "Lockdown recovery kit"));
    }

    #[test]
    fn both_values_appear_exactly_once_in_groups_of_four() {
        let pdf = render_page();
        let shown = parse(&pdf).unwrap().shown;
        let all: String = shown.concat();
        for typed in [SK_TYPED, RC_TYPED] {
            assert_eq!(all.matches(typed).count(), 1, "{typed} once, grouped");
            let groups: Vec<&str> = typed
                .as_bytes()
                .chunks(4)
                .map(|c| std::str::from_utf8(c).unwrap())
                .collect();
            // Each group is its own run, in order, one after the other.
            let at = shown.iter().position(|s| s == groups[0]).unwrap();
            assert_eq!(&shown[at..at + groups.len()], groups.as_slice());
        }
        // The printed forms with dashes are not on the page a second time.
        assert!(!all.contains("A3-K7QF2"));
        assert!(!all.contains("RC4F-9K2M"));
        // Nor is either value anywhere in the file outside the content stream.
        let raw = String::from_utf8_lossy(&pdf);
        assert_eq!(raw.matches("A3K7").count(), 1);
    }

    #[test]
    fn the_page_carries_the_date_the_lines_and_where_to_keep_it() {
        let shown = parse(&render_page()).unwrap().shown;
        // Wrapped lines rejoin with a space, as a reader reads them.
        let text = shown.join(" ");
        assert!(text.contains("Made 2026-10-29 on Sam’s MacBook"));
        assert!(text.contains("2026-10-29"));
        assert!(text.contains("sam@rivera.example"));
        assert!(text.contains("https://vault.rivera.example"));
        assert!(text.contains("the account's slot now needs this Secret Key"));
        assert!(text.contains("Where to keep this file"));
        assert!(text.contains("Lockdown keeps no copy."));
        assert!(text.contains("USB drive"));
        assert!(text.contains("Keep this page somewhere safe"));
        assert!(text.contains("Lockdown never asks for this page"));
    }

    #[test]
    fn the_same_page_gives_the_same_bytes() {
        assert_eq!(*render_page(), *render_page());
    }

    #[test]
    fn anything_but_a_kit_page_is_refused() {
        assert!(render(&[], "hello").is_err());
        assert!(render(&[], "LOCKDOWN RECOVERY KIT\nSecret Key: A3-XXXX\n").is_err());
        let twice = PAGE.replace("Server:", "Secret Key:     A3-K7QF2\nServer:");
        assert!(render(&[], &twice).is_err());
        let odd = PAGE.replace("A3-K7QF2", "A3-K7Q(F2");
        assert!(render(&[], &odd).is_err());
        let big = format!("{PAGE}{}", "x".repeat(MAX_PAGE));
        assert!(render(&[], &big).is_err());
        assert!(render(&vec!["note".to_string(); MAX_NOTES + 1], PAGE).is_err());
        assert!(check(PAGE).is_ok());
    }

    #[test]
    fn text_outside_win_ansi_prints_as_a_question_mark() {
        assert_eq!(*win_ansi("é’€ok"), [0xE9, 0x92, 0x80, b'o', b'k']);
        assert_eq!(*win_ansi("名"), [b'?']);
    }

    #[test]
    fn wrapping_keeps_every_word() {
        let text = "one two three four five six seven eight nine ten";
        let lines = wrap(text, 12);
        assert!(lines.iter().all(|l| l.chars().count() <= 12));
        assert_eq!(lines.join(" "), text);
        assert_eq!(wrap(&"x".repeat(20), 8).concat(), "x".repeat(20));
    }

    /// The constants above are the fonts' own: 1000 units to the em, every
    /// glyph 600 wide, and every WinAnsi character has a glyph (no box).
    #[test]
    fn the_embedded_fonts_match_the_metrics_written() {
        for font in [REGULAR, BOLD] {
            let t = Tables::read(font);
            let head = t.table(b"head");
            assert_eq!(u16_at(head, 18), 1000);
            let hhea = t.table(b"hhea");
            assert_eq!(u16_at(hhea, 4) as i16, 1020);
            assert_eq!(u16_at(hhea, 6) as i16, -300);
            let metrics = u16_at(hhea, 34) as usize;
            let hmtx = t.table(b"hmtx");
            for g in 1..metrics {
                let w = u16_at(hmtx, g * 4);
                assert!(w == 600 || w == 0, "glyph {g} is {w} wide");
            }
            let os2 = t.table(b"OS/2");
            assert_eq!(u16_at(os2, 88), 730);
            let cmap = t.cmap();
            for b in (0x20u8..=0x7E).chain(0xA0..=0xFF) {
                assert!(cmap(u32::from(b)) != 0, "no glyph for {b:#x}");
            }
            for c in ['€', '’', '“', '”', '–', '—', '…'] {
                assert!(cmap(c as u32) != 0, "no glyph for {c}");
            }
        }
    }

    fn u16_at(b: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([b[at], b[at + 1]])
    }

    fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    struct Tables<'a>(&'a [u8]);

    impl<'a> Tables<'a> {
        fn read(font: &'a [u8]) -> Self {
            Self(font)
        }

        fn table(&self, tag: &[u8; 4]) -> &'a [u8] {
            let n = u16_at(self.0, 4) as usize;
            (0..n)
                .map(|i| 12 + i * 16)
                .find(|r| &self.0[*r..*r + 4] == tag)
                .map(|r| {
                    let off = u32_at(self.0, r + 8) as usize;
                    let len = u32_at(self.0, r + 12) as usize;
                    &self.0[off..off + len]
                })
                .unwrap_or_else(|| panic!("no {} table", String::from_utf8_lossy(tag)))
        }

        /// The Windows Unicode BMP map (3, 1, format 4) as a lookup.
        fn cmap(&self) -> impl Fn(u32) -> u16 + 'a {
            let cmap = self.table(b"cmap");
            let n = u16_at(cmap, 2) as usize;
            let sub = (0..n)
                .map(|i| 4 + i * 8)
                .find(|r| u16_at(cmap, *r) == 3 && u16_at(cmap, r + 2) == 1)
                .map(|r| &cmap[u32_at(cmap, r + 4) as usize..])
                .expect("a Windows Unicode map");
            assert_eq!(u16_at(sub, 0), 4);
            move |c: u32| {
                let segs = u16_at(sub, 6) as usize / 2;
                let ends = 14;
                let starts = ends + segs * 2 + 2;
                let deltas = starts + segs * 2;
                let ranges = deltas + segs * 2;
                for s in 0..segs {
                    let end = u32::from(u16_at(sub, ends + s * 2));
                    let start = u32::from(u16_at(sub, starts + s * 2));
                    if c < start || c > end {
                        continue;
                    }
                    let delta = u16_at(sub, deltas + s * 2);
                    let range = u16_at(sub, ranges + s * 2) as usize;
                    if range == 0 {
                        return (c as u16).wrapping_add(delta);
                    }
                    let at = ranges + s * 2 + range + (c - start) as usize * 2;
                    let g = u16_at(sub, at);
                    return if g == 0 { 0 } else { g.wrapping_add(delta) };
                }
                0
            }
        }
    }
}
