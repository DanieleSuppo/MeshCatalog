use rusqlite::{Connection, TransactionBehavior};

use crate::error::IpcError;

const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/001_initial_schema.sql"),
    include_str!("../../migrations/002_allow_unassociated_images.sql"),
];

pub(super) fn run(connection: &mut Connection) -> Result<(), IpcError> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let start = usize::try_from(version).map_err(|_| IpcError::UnsupportedSchema(version))?;
    let pending = MIGRATIONS
        .get(start..)
        .ok_or(IpcError::UnsupportedSchema(version))?;

    for sql in pending {
        tx.execute_batch(sql)?;
    }
    if !pending.is_empty() {
        tx.pragma_update(None, "user_version", MIGRATIONS.len() as i64)?;
    }
    tx.commit()?;
    Ok(())
}
