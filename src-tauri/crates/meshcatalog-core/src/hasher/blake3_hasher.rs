use std::io::{self, Read};

use tokio_util::sync::CancellationToken;

pub(crate) fn content_hash(reader: impl Read, cancel: &CancellationToken) -> io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    // update_reader streams with BLAKE3's 64 KiB read buffer.
    hasher.update_reader(CancellableReader(reader, cancel))?;
    Ok(hasher.finalize().to_hex().to_string())
}

struct CancellableReader<'a, R>(R, &'a CancellationToken);

impl<R: Read> Read for CancellableReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.1.is_cancelled() {
            // update_reader retries Interrupted; use a terminal error for cancellation.
            return Err(io::Error::other("Scansione annullata"));
        }
        self.0.read(buf)
    }
}
