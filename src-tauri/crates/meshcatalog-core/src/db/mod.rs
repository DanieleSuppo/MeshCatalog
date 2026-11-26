mod migrations;
mod queries;
pub(crate) mod sources;
pub(crate) mod writer;

use std::{
    path::Path,
    sync::{Condvar, Mutex, PoisonError},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags};

use crate::error::IpcError;

const READER_COUNT: usize = 4;

pub struct Database {
    writer: Mutex<Connection>,
    readers: Mutex<Vec<Connection>>,
    reader_available: Condvar,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, IpcError> {
        let mut writer = Connection::open(path)?;
        configure(&writer)?;
        migrations::run(&mut writer)?;
        let journal: String =
            writer.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        if journal != "wal" {
            return Err(IpcError::WalUnavailable);
        }

        let mut readers = Vec::with_capacity(READER_COUNT);
        for _ in 0..READER_COUNT {
            let reader = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            configure(&reader)?;
            readers.push(reader);
        }

        Ok(Self {
            writer: Mutex::new(writer),
            readers: Mutex::new(readers),
            reader_available: Condvar::new(),
        })
    }

    pub fn with_writer<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, IpcError>,
    ) -> Result<T, IpcError> {
        let mut writer = self.writer.lock().map_err(|_| IpcError::InternalError)?;
        operation(&mut writer)
    }

    pub fn with_reader<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, IpcError>,
    ) -> Result<T, IpcError> {
        let mut available = self.readers.lock().unwrap_or_else(PoisonError::into_inner);
        while available.is_empty() {
            available = self
                .reader_available
                .wait(available)
                .unwrap_or_else(PoisonError::into_inner);
        }
        let reader = ReaderLease {
            db: self,
            connection: available.pop(),
        };
        drop(available);
        operation(reader.connection.as_ref().expect("reader was checked out"))
    }
}

struct ReaderLease<'a> {
    db: &'a Database,
    connection: Option<Connection>,
}

impl Drop for ReaderLease<'_> {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            self.db
                .readers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(connection);
            self.db.reader_available.notify_one();
        }
    }
}

fn configure(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(Duration::from_millis(5000))?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    // Keep SQLite sort and temporary table files out of the OS temp directory.
    connection.pragma_update(None, "temp_store", "MEMORY")
}
