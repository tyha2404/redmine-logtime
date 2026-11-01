use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Result;
use crate::model::{Batch, Op, OpPayload, OpStatus};

const SCHEMA: &str = "
-- The review queue of earlier versions; writes now go straight to Redmine.
DROP TABLE IF EXISTS draft;
CREATE TABLE IF NOT EXISTS batches (
    id          INTEGER PRIMARY KEY,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    label       TEXT NOT NULL,
    redmine_url TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS ops (
    id        INTEGER PRIMARY KEY,
    batch_id  INTEGER NOT NULL REFERENCES batches(id),
    seq       INTEGER NOT NULL,
    payload   TEXT NOT NULL,
    status    TEXT NOT NULL,
    result_id INTEGER,
    error     TEXT
);
CREATE INDEX IF NOT EXISTS ops_batch ON ops(batch_id, seq);
CREATE TABLE IF NOT EXISTS kv (
    key  TEXT PRIMARY KEY,
    json TEXT NOT NULL
);
-- Sub-tasks this app created, keyed by SubtaskDraft::key (idempotency).
CREATE TABLE IF NOT EXISTS subtasks (
    key      TEXT PRIMARY KEY,
    issue_id INTEGER NOT NULL,
    batch_id INTEGER
);
";

/// Local SQLite store: write batches (the audit trail used by undo and retry).
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Small JSON documents (Redmine profile, last report…).
    pub fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT json FROM kv WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(raw.map(|r| serde_json::from_str(&r)).transpose()?)
    }

    pub fn set_json<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO kv (key, json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET json = excluded.json",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    pub fn create_batch(&self, label: &str, redmine_url: &str, ops: &[OpPayload]) -> Result<i64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO batches (label, redmine_url) VALUES (?1, ?2)",
            params![label, redmine_url],
        )?;
        let batch_id = tx.last_insert_rowid();
        for (seq, op) in ops.iter().enumerate() {
            tx.execute(
                "INSERT INTO ops (batch_id, seq, payload, status) VALUES (?1, ?2, ?3, 'pending')",
                params![batch_id, seq as i64, serde_json::to_string(op)?],
            )?;
        }
        tx.commit()?;
        Ok(batch_id)
    }

    pub fn batch(&self, id: i64) -> Result<Batch> {
        let conn = self.conn.lock().unwrap();
        let mut batch = conn.query_row(
            "SELECT id, created_at, label, redmine_url FROM batches WHERE id = ?1",
            params![id],
            |r| {
                Ok(Batch {
                    id: r.get(0)?,
                    created_at: r.get(1)?,
                    label: r.get(2)?,
                    redmine_url: r.get(3)?,
                    ops: Vec::new(),
                })
            },
        )?;
        batch.ops = load_ops(&conn, id)?;
        Ok(batch)
    }

    /// Most recent batches first.
    pub fn list_batches(&self, limit: u32) -> Result<Vec<Batch>> {
        let ids: Vec<i64> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT id FROM batches ORDER BY id DESC LIMIT ?1")?;
            stmt.query_map(params![limit], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        ids.into_iter().map(|id| self.batch(id)).collect()
    }

    pub fn set_op(
        &self,
        op_id: i64,
        status: OpStatus,
        result_id: Option<u32>,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE ops SET status = ?2, result_id = COALESCE(?3, result_id), error = ?4 WHERE id = ?1",
            params![op_id, status.as_str(), result_id, error],
        )?;
        Ok(())
    }

    /// Records that a failed request may still have reached Redmine (lost
    /// connection, timeout, 5xx), so a retry must look before creating.
    pub fn mark_uncertain(&self, op_id: i64) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("UPDATE ops SET uncertain = 1 WHERE id = ?1", params![op_id])?;
        Ok(())
    }

    /// Puts a batch's failed ops back so they can run again. Ops whose
    /// failure was uncertain come back as interrupted (`running`): the next
    /// run adopts what Redmine already has instead of creating it twice.
    pub fn reset_failed(&self, batch_id: i64) -> Result<usize> {
        Ok(self.conn.lock().unwrap().execute(
            "UPDATE ops SET status = CASE WHEN uncertain = 1 THEN 'running' ELSE 'pending' END, error = NULL
             WHERE batch_id = ?1 AND status = 'failed'",
            params![batch_id],
        )?)
    }

    pub fn subtask_issue(&self, key: &str) -> Result<Option<u32>> {
        Ok(self
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT issue_id FROM subtasks WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn save_subtask(&self, key: &str, issue_id: u32, batch_id: i64) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO subtasks (key, issue_id, batch_id) VALUES (?1, ?2, ?3)",
            params![key, issue_id, batch_id],
        )?;
        Ok(())
    }
}

/// Columns added after the first release.
fn migrate(conn: &Connection) -> Result<()> {
    let has_uncertain = conn
        .prepare("SELECT 1 FROM pragma_table_info('ops') WHERE name = 'uncertain'")?
        .exists([])?;
    if !has_uncertain {
        conn.execute_batch("ALTER TABLE ops ADD COLUMN uncertain INTEGER NOT NULL DEFAULT 0")?;
    }
    Ok(())
}

fn load_ops(conn: &Connection, batch_id: i64) -> Result<Vec<Op>> {
    let mut stmt = conn.prepare(
        "SELECT id, seq, payload, status, result_id, error FROM ops WHERE batch_id = ?1 ORDER BY seq",
    )?;
    let rows = stmt.query_map(params![batch_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<u32>>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut ops = Vec::new();
    for row in rows {
        let (id, seq, payload, status, result_id, error) = row?;
        ops.push(Op {
            id,
            seq,
            payload: serde_json::from_str(&payload)?,
            status: OpStatus::parse(&status),
            result_id,
            error,
        });
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_from_the_first_release_gains_the_uncertain_column() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ops (id INTEGER PRIMARY KEY, batch_id INTEGER NOT NULL, seq INTEGER NOT NULL,
                               payload TEXT NOT NULL, status TEXT NOT NULL, result_id INTEGER, error TEXT);
             INSERT INTO ops (batch_id, seq, payload, status) VALUES (1, 0, '{}', 'failed');",
        )
        .unwrap();
        let store = Store::init(conn).unwrap();
        let conn = store.conn.lock().unwrap();
        let uncertain: i64 = conn
            .query_row("SELECT uncertain FROM ops", [], |r| r.get(0))
            .unwrap();
        assert_eq!(uncertain, 0);
        // Opening again does not try to add it twice.
        migrate(&conn).unwrap();
    }
}
