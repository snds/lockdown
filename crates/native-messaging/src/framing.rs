//! The browser's native-messaging framing: each message is a 32-bit length
//! in the machine's own byte order, then that many bytes of UTF-8 JSON.
//! Chromium refuses a message from the host over 1 MiB; this channel holds
//! both directions to that, so a page that makes the extension send a huge
//! message cannot make the host buffer it.

use std::io::{ErrorKind, Read, Write};

use crate::{Error, Result};

/// The largest message either way: Chromium's limit for host → browser.
pub const MAX_MESSAGE: usize = 1024 * 1024;

/// Reads one message. `Ok(None)` is a clean end of input (the browser
/// closed the port); a stream that stops inside a message is an error.
pub fn read_message<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    let mut head = [0u8; 4];
    match reader.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_ne_bytes(head) as usize;
    if len > MAX_MESSAGE {
        return Err(Error::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    if std::str::from_utf8(&body).is_err() {
        return Err(Error::BadMessage("not UTF-8".into()));
    }
    Ok(Some(body))
}

/// Writes one message and flushes.
pub fn write_message<W: Write>(writer: &mut W, body: &[u8]) -> Result<()> {
    if body.len() > MAX_MESSAGE {
        return Err(Error::TooLarge(body.len()));
    }
    writer.write_all(&(body.len() as u32).to_ne_bytes())?;
    writer.write_all(body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_round_trips_and_end_of_input_is_clean() {
        let mut buf = Vec::new();
        write_message(&mut buf, br#"{"id":1,"op":"status"}"#).unwrap();
        write_message(&mut buf, b"{}").unwrap();
        assert_eq!(&buf[..4], &22u32.to_ne_bytes());
        let mut r = buf.as_slice();
        assert_eq!(
            read_message(&mut r).unwrap().unwrap(),
            br#"{"id":1,"op":"status"}"#
        );
        assert_eq!(read_message(&mut r).unwrap().unwrap(), b"{}");
        assert!(read_message(&mut r).unwrap().is_none());
    }

    #[test]
    fn oversized_truncated_and_binary_messages_are_refused() {
        let big = ((MAX_MESSAGE + 1) as u32).to_ne_bytes();
        assert!(matches!(
            read_message(&mut big.as_slice()),
            Err(Error::TooLarge(_))
        ));
        assert!(matches!(
            write_message(&mut Vec::new(), &vec![b' '; MAX_MESSAGE + 1]),
            Err(Error::TooLarge(_))
        ));
        let mut cut = 10u32.to_ne_bytes().to_vec();
        cut.extend_from_slice(b"{\"a\"");
        assert!(matches!(
            read_message(&mut cut.as_slice()),
            Err(Error::Io(_))
        ));
        let mut bin = 2u32.to_ne_bytes().to_vec();
        bin.extend_from_slice(&[0xff, 0xfe]);
        assert!(matches!(
            read_message(&mut bin.as_slice()),
            Err(Error::BadMessage(_))
        ));
    }
}
