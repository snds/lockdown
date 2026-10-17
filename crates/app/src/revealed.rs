//! A secret on its way to a shell: bytes, never a string, wiped on drop
//! (03-architecture §8). A shell copies them into its own buffer, shows or
//! pastes them, and wipes that buffer too; nothing here can be printed by
//! accident, because there is no `Display` and `Debug` prints the length.

use zeroize::Zeroize;

/// Secret bytes handed to a shell for one use.
pub struct Revealed {
    bytes: Vec<u8>,
}

impl Revealed {
    /// Takes ownership of `bytes`; they are wiped when this value drops.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// From text (a password, the printed kit). The source string is the
    /// caller's to wipe.
    pub fn from_text(text: &str) -> Self {
        Self::new(text.as_bytes().to_vec())
    }

    /// The bytes. Use them at once; never copy them into a `String` that
    /// outlives this value.
    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }

    /// How many bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Hands the bytes to the caller, who now owns the wiping. The one way
    /// out, for a shell that must give a byte buffer to its bridge.
    pub fn into_bytes(mut self) -> zeroize::Zeroizing<Vec<u8>> {
        zeroize::Zeroizing::new(std::mem::take(&mut self.bytes))
    }

    /// Overwrites the bytes with zeros now, keeping the length, so a test
    /// (or a shell that is done early) can check nothing is left.
    pub fn wipe(&mut self) {
        self.bytes.as_mut_slice().zeroize();
    }
}

impl Drop for Revealed {
    fn drop(&mut self) {
        self.wipe();
        #[cfg(test)]
        tests::note_drop(self.bytes.iter().all(|b| *b == 0));
    }
}

impl std::fmt::Debug for Revealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Revealed(<{} bytes>)", self.bytes.len())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::Cell;

    // Per thread, so the other tests' drops (on their own threads and on
    // the app's worker) don't count here.
    thread_local! {
        static DROPS: Cell<usize> = const { Cell::new(0) };
        static LAST_DROP_WAS_ZERO: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) fn note_drop(all_zero: bool) {
        DROPS.with(|d| d.set(d.get() + 1));
        LAST_DROP_WAS_ZERO.with(|z| z.set(all_zero));
    }

    #[test]
    fn the_buffer_is_zeroed_before_it_is_freed() {
        let before = DROPS.with(Cell::get);
        {
            let r = Revealed::from_text("correct horse battery staple");
            assert_eq!(r.expose(), b"correct horse battery staple");
        }
        assert_eq!(DROPS.with(Cell::get), before + 1);
        assert!(LAST_DROP_WAS_ZERO.with(Cell::get));
    }

    #[test]
    fn wipe_keeps_the_length_and_zeroes_every_byte() {
        let mut r = Revealed::new(vec![7u8; 12]);
        r.wipe();
        assert_eq!(r.len(), 12);
        assert!(r.expose().iter().all(|b| *b == 0));
    }

    #[test]
    fn debug_prints_only_the_length() {
        let r = Revealed::from_text("hunter2");
        assert_eq!(format!("{r:?}"), "Revealed(<7 bytes>)");
    }
}
