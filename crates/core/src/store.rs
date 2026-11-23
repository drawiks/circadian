use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::model::{
    Gap, GapDraft, PersistedState, PowerSource, Session, SessionDraft, SessionKind, Settings,
};
use crate::time::now_utc;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("сессия пересекается с существующей")]
    Overlap,
    #[error("сессия не найдена")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store {
    conn: Connection,
    path: PathBuf,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { conn, path };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start_utc INTEGER NOT NULL,
                end_utc INTEGER NOT NULL,
                start_offset_min INTEGER NOT NULL,
                end_offset_min INTEGER NOT NULL,
                kind TEXT NOT NULL,
                source TEXT NOT NULL,
                confidence INTEGER NOT NULL,
                confirmed INTEGER NOT NULL,
                note TEXT NOT NULL DEFAULT '',
                created_utc INTEGER NOT NULL,
                updated_utc INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_end ON sessions(end_utc);

            CREATE TABLE IF NOT EXISTS gaps (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start_utc INTEGER NOT NULL,
                end_utc INTEGER NOT NULL,
                reason TEXT NOT NULL,
                created_utc INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_gaps_end ON gaps(end_utc);

            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                at_utc INTEGER NOT NULL,
                kind TEXT NOT NULL,
                detail TEXT NOT NULL DEFAULT ''
            );
            "#,
        )?;

        let settings = serde_json::to_string(&Settings::default())?;
        self.conn.execute(
            "INSERT OR IGNORE INTO settings (id, json) VALUES (1, ?1)",
            params![settings],
        )?;
        let state = serde_json::to_string(&PersistedState::default())?;
        self.conn.execute(
            "INSERT OR IGNORE INTO state (id, json) VALUES (1, ?1)",
            params![state],
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> Result<Settings> {
        let json: String =
            self.conn
                .query_row("SELECT json FROM settings WHERE id = 1", [], |row| {
                    row.get(0)
                })?;
        Ok(serde_json::from_str(&json)?)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings)?;
        self.conn
            .execute("UPDATE settings SET json = ?1 WHERE id = 1", params![json])?;
        Ok(())
    }

    pub fn load_state(&self) -> Result<PersistedState> {
        let json: String =
            self.conn
                .query_row("SELECT json FROM state WHERE id = 1", [], |row| row.get(0))?;
        Ok(serde_json::from_str(&json)?)
    }

    pub fn save_state(&self, state: &PersistedState) -> Result<()> {
        let json = serde_json::to_string(state)?;
        self.conn
            .execute("UPDATE state SET json = ?1 WHERE id = 1", params![json])?;
        Ok(())
    }

    pub fn update_last_seen(&self, state: &PersistedState) -> Result<()> {
        self.save_state(state)
    }

    pub fn overlaps(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        skip_id: Option<i64>,
    ) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions
             WHERE start_utc < ?2 AND end_utc > ?1 AND id != ?3",
            params![start.timestamp(), end.timestamp(), skip_id.unwrap_or(-1)],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn insert_session(&self, draft: &SessionDraft) -> Result<i64> {
        self.insert_session_inner(draft, false)
            .map(|id| id.expect("checked insert returns id"))
    }

    pub fn try_insert_session(&self, draft: &SessionDraft) -> Result<Option<i64>> {
        self.insert_session_inner(draft, true)
    }

    fn insert_session_inner(
        &self,
        draft: &SessionDraft,
        skip_on_overlap: bool,
    ) -> Result<Option<i64>> {
        if draft.end_utc <= draft.start_utc {
            return Err(StoreError::Overlap);
        }
        if self.overlaps(draft.start_utc, draft.end_utc, None)? {
            if skip_on_overlap {
                return Ok(None);
            }
            return Err(StoreError::Overlap);
        }
        let now = now_utc();
        self.conn.execute(
            "INSERT INTO sessions
             (start_utc, end_utc, start_offset_min, end_offset_min, kind, source,
              confidence, confirmed, note, created_utc, updated_utc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                draft.start_utc.timestamp(),
                draft.end_utc.timestamp(),
                draft.start_offset_min,
                draft.end_offset_min,
                draft.kind.as_str(),
                draft.source.as_str(),
                draft.confidence as i64,
                draft.confirmed as i64,
                draft.note,
                now.timestamp(),
                now.timestamp(),
            ],
        )?;
        Ok(Some(self.conn.last_insert_rowid()))
    }

    pub fn update_session(&self, id: i64, draft: &SessionDraft) -> Result<()> {
        if draft.end_utc <= draft.start_utc {
            return Err(StoreError::Overlap);
        }
        if self.overlaps(draft.start_utc, draft.end_utc, Some(id))? {
            return Err(StoreError::Overlap);
        }
        let changed = self.conn.execute(
            "UPDATE sessions SET
                start_utc = ?1, end_utc = ?2, start_offset_min = ?3, end_offset_min = ?4,
                kind = ?5, source = ?6, confidence = ?7, confirmed = ?8, note = ?9,
                updated_utc = ?10
             WHERE id = ?11",
            params![
                draft.start_utc.timestamp(),
                draft.end_utc.timestamp(),
                draft.start_offset_min,
                draft.end_offset_min,
                draft.kind.as_str(),
                draft.source.as_str(),
                draft.confidence as i64,
                draft.confirmed as i64,
                draft.note,
                now_utc().timestamp(),
                id,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn delete_session(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn set_confirmed(&self, id: i64, confirmed: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET confirmed = ?1, updated_utc = ?2 WHERE id = ?3",
            params![confirmed as i64, now_utc().timestamp(), id],
        )?;
        Ok(())
    }

    pub fn set_kind(&self, id: i64, kind: SessionKind, confirmed: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET kind = ?1, confirmed = ?2, updated_utc = ?3 WHERE id = ?4",
            params![kind.as_str(), confirmed as i64, now_utc().timestamp(), id],
        )?;
        Ok(())
    }

    pub fn all_sessions(&self) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_utc, end_utc, start_offset_min, end_offset_min, kind, source,
                    confidence, confirmed, note, created_utc, updated_utc
             FROM sessions ORDER BY end_utc DESC, start_utc DESC",
        )?;
        let rows = stmt.query_map([], session_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn sessions_between(
        &self,
        from_utc: DateTime<Utc>,
        to_utc: DateTime<Utc>,
    ) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_utc, end_utc, start_offset_min, end_offset_min, kind, source,
                    confidence, confirmed, note, created_utc, updated_utc
             FROM sessions
             WHERE end_utc >= ?1 AND end_utc < ?2
             ORDER BY end_utc ASC, start_utc ASC",
        )?;
        let rows = stmt.query_map(
            params![from_utc.timestamp(), to_utc.timestamp()],
            session_from_row,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn insert_gap_if_new(&self, gap: &GapDraft) -> Result<bool> {
        if gap.end_utc <= gap.start_utc {
            return Ok(false);
        }
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM gaps WHERE start_utc < ?2 AND end_utc > ?1",
            params![gap.start_utc.timestamp(), gap.end_utc.timestamp()],
            |row| row.get(0),
        )?;
        if exists > 0 {
            return Ok(false);
        }
        self.conn.execute(
            "INSERT INTO gaps (start_utc, end_utc, reason, created_utc)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                gap.start_utc.timestamp(),
                gap.end_utc.timestamp(),
                gap.reason,
                now_utc().timestamp()
            ],
        )?;
        Ok(true)
    }

    pub fn gaps_between(&self, from_utc: DateTime<Utc>, to_utc: DateTime<Utc>) -> Result<Vec<Gap>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_utc, end_utc, reason FROM gaps
             WHERE end_utc >= ?1 AND end_utc < ?2 ORDER BY end_utc ASC",
        )?;
        let rows = stmt.query_map(params![from_utc.timestamp(), to_utc.timestamp()], |row| {
            Ok(Gap {
                id: row.get(0)?,
                start_utc: ts(row.get(1)?),
                end_utc: ts(row.get(2)?),
                reason: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn insert_event(&self, at: DateTime<Utc>, kind: &str, detail: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO events (at_utc, kind, detail) VALUES (?1, ?2, ?3)",
            params![at.timestamp(), kind, detail],
        )?;
        Ok(())
    }

    pub fn data_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA data_version", [], |row| row.get(0))?)
    }

    pub fn pending_count(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE confirmed = 0",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn session_by_id(&self, id: i64) -> Result<Option<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_utc, end_utc, start_offset_min, end_offset_min, kind, source,
                    confidence, confirmed, note, created_utc, updated_utc
             FROM sessions WHERE id = ?1",
        )?;
        let row = stmt.query_row(params![id], session_from_row).optional()?;
        Ok(row)
    }
}

fn ts(value: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(value, 0).single().unwrap_or_else(|| {
        Utc.timestamp_opt(0, 0)
            .single()
            .expect("unix epoch is valid")
    })
}

fn session_from_row(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        start_utc: ts(row.get(1)?),
        end_utc: ts(row.get(2)?),
        start_offset_min: row.get(3)?,
        end_offset_min: row.get(4)?,
        kind: SessionKind::from_db(&row.get::<_, String>(5)?),
        source: PowerSource::from_db(&row.get::<_, String>(6)?),
        confidence: row.get::<_, i64>(7)?.clamp(0, 100) as u8,
        confirmed: row.get::<_, i64>(8)? != 0,
        note: row.get(9)?,
        created_utc: ts(row.get(10)?),
        updated_utc: ts(row.get(11)?),
    })
}
