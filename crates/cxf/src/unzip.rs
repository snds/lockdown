//! A ZIP file read into memory under caps: how many entries, how big each
//! one inflates, how big all of them inflate together. A ZIP bomb stops at
//! the first cap it crosses, not when memory runs out; entry names are
//! checked (no absolute paths, no `..`) even though nothing here is ever
//! written to disk.

use std::io::{Cursor, Read};

use zeroize::Zeroizing;

use crate::{CxfError, Limits, Result};

/// One file from the ZIP: its path and its bytes (wiped when dropped).
pub struct Entry {
    /// The path inside the ZIP, with `/` separators.
    pub name: String,
    /// The bytes.
    pub bytes: Zeroizing<Vec<u8>>,
}

/// Whether `bytes` start like a ZIP file.
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

fn bad(why: impl Into<String>) -> CxfError {
    CxfError::Parse {
        format: "ZIP",
        why: why.into(),
    }
}

/// Every file in the ZIP, in order, within `limits`.
pub fn read(bytes: &[u8], limits: &Limits) -> Result<Vec<Entry>> {
    limits.check_size(bytes)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| bad(e.to_string()))?;
    if archive.len() > limits.max_entries {
        return Err(CxfError::TooMany {
            what: "files in the ZIP",
            found: archive.len(),
            limit: limits.max_entries,
        });
    }
    let mut out = Vec::new();
    let mut total = 0usize;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| bad(e.to_string()))?;
        if file.is_dir() {
            continue;
        }
        let name = file
            .name()
            .map_err(|e| bad(e.to_string()))?
            .replace('\\', "/");
        if name.starts_with('/') || name.split('/').any(|p| p == "..") || name.contains(':') {
            return Err(bad(format!("an entry has an unsafe path: {name}")));
        }
        let cap = limits.max_attachment_bytes.max(limits.max_field_bytes);
        let cap = cap.max(limits.max_bytes);
        if file.size() > cap as u64 {
            return Err(CxfError::TooLarge {
                found: usize::try_from(file.size()).unwrap_or(usize::MAX),
                limit: cap,
            });
        }
        let mut buf = Zeroizing::new(Vec::new());
        let read = (&mut file)
            .take(cap as u64 + 1)
            .read_to_end(&mut buf)
            .map_err(|e| bad(format!("{name} didn't unpack: {e}")))?;
        if read > cap {
            return Err(CxfError::TooLarge {
                found: read,
                limit: cap,
            });
        }
        total += read;
        if total > limits.max_unpacked_bytes {
            return Err(CxfError::TooLarge {
                found: total,
                limit: limits.max_unpacked_bytes,
            });
        }
        out.push(Entry { name, bytes: buf });
    }
    Ok(out)
}

/// A ZIP of `files` (stored, not compressed): for tests and fixtures.
#[cfg(test)]
pub fn write(files: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        w.start_file(*name, opts).unwrap();
        w.write_all(bytes).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_hold_and_bad_paths_are_refused() {
        let zip = write(&[("a.txt", b"hello"), ("dir/b.json", b"{}")]);
        assert!(is_zip(&zip));
        let files = read(&zip, &Limits::default()).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[1].name, "dir/b.json");
        // A bomb: one entry that inflates past the cap.
        let big = vec![0u8; 200_000];
        let bomb = write(&[("zeros", &big)]);
        assert!(bomb.len() < 10_000, "deflate packs zeros small");
        let tight = Limits {
            max_bytes: 100_000,
            max_attachment_bytes: 1_000,
            max_field_bytes: 1_000,
            ..Limits::default()
        };
        assert!(matches!(
            read(&bomb, &tight),
            Err(CxfError::TooLarge { .. })
        ));
        let many = Limits {
            max_entries: 1,
            ..Limits::default()
        };
        assert!(matches!(read(&zip, &many), Err(CxfError::TooMany { .. })));
        let evil = write(&[("../../etc/passwd", b"x")]);
        assert!(read(&evil, &Limits::default()).is_err());
        // Garbage that starts like a ZIP is an error, not a panic.
        let mut cut = zip.clone();
        cut.truncate(cut.len() / 2);
        assert!(read(&cut, &Limits::default()).is_err());
        assert!(read(b"PK\x03\x04garbage", &Limits::default()).is_err());
    }
}
