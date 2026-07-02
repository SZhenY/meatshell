//! A single, size-capped diagnostic log file (#86).
//!
//! Writes go to `<log_dir>/error.log` — its own `log/` folder beside the exe
//! (portable-first; see [`crate::config::log_dir`]), kept separate from the
//! config dir. The file is capped at a fixed size:
//! when the next write would exceed the cap it is truncated to empty and writing
//! restarts from the top — so there is always exactly one file, at most `cap`
//! bytes, that auto-overwrites its old content. This lets users (e.g. behind a
//! bastion) send their disconnect reason without setting RUST_LOG.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::Mutex;

/// `<log_dir>/error.log`, in its own `log/` folder beside the exe.
pub fn path() -> Option<PathBuf> {
    let dir = crate::config::log_dir();
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("error.log"))
}

/// One log file capped at `cap` bytes (truncate-and-restart when full).
pub struct CappedFile {
    path: PathBuf,
    file: File,
    written: u64,
    cap: u64,
}

impl CappedFile {
    pub fn open(path: PathBuf, cap: u64) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            file,
            written,
            cap,
        })
    }
}

impl Write for CappedFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written.saturating_add(buf.len() as u64) > self.cap {
            // Truncate to empty and start over so we never exceed the cap.
            self.file = File::create(&self.path)?;
            self.written = 0;
        }
        let n = self.file.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// `MakeWriter` over a shared [`CappedFile`] so a tracing fmt layer can use it.
#[derive(Clone)]
pub struct CappedWriter(Arc<Mutex<CappedFile>>);

impl CappedWriter {
    pub fn new(cf: CappedFile) -> Self {
        Self(Arc::new(Mutex::new(cf)))
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CappedWriter {
    type Writer = Guard<'a>;
    fn make_writer(&'a self) -> Self::Writer {
        Guard(self.0.lock())
    }
}

pub struct Guard<'a>(parking_lot::MutexGuard<'a, CappedFile>);

impl Write for Guard<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
