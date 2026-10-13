//! `mobile.sqlite`: what the phone keeps between launches, as JSON values under fixed keys.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result};
use rusqlite::{Connection, OptionalExtension as _, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS state (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
";

/// The queue, as `queue::Queue` serialises it.
pub const QUEUE: &str = "queue";
/// Where the current track was, in milliseconds.
pub const POSITION: &str = "position";
/// What every key holding a copy of a provider's data starts with, followed by the provider's
/// slug and a colon, so a sign-out can drop them all at once.
pub const SNAPSHOT: &str = "snapshot:";
/// How long a write waits for the other connection to the database to finish its own.
const BUSY: Duration = Duration::from_secs(2);

/// An open connection to the database. The player keeps one for the queue and the library
/// another for its snapshots.
pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let connection =
            Connection::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        connection
            .busy_timeout(BUSY)
            .context("cannot set the busy timeout")?;
        connection
            .execute_batch(SCHEMA)
            .context("cannot create the mobile schema")?;
        Ok(Self { connection })
    }

    /// The value under `key`, or `None` when there is none or it no longer reads.
    pub fn load<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let text: Option<String> = self
            .connection
            .query_row("SELECT value FROM state WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .unwrap_or_else(|error| {
                log::warn!("store: cannot read {key}: {error}");
                None
            });
        serde_json::from_str(&text?)
            .inspect_err(|error| log::warn!("store: cannot parse {key}: {error}"))
            .ok()
    }

    /// Writes `value` under `key`. A failure is logged, never fatal: losing a save only
    /// costs the next launch its restore.
    pub fn save<T: Serialize>(&self, key: &str, value: &T) {
        let written = serde_json::to_string(value)
            .context("cannot serialise")
            .and_then(|text| {
                self.connection
                    .execute(
                        "INSERT INTO state (key, value) VALUES (?1, ?2)
                         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                        params![key, text],
                    )
                    .context("cannot write")
            });
        if let Err(error) = written {
            log::warn!("store: cannot save {key}: {error:#}");
        }
    }

    /// The start of every key holding a copy of the data of the provider `slug`.
    pub fn snapshots(slug: &str) -> String {
        format!("{SNAPSHOT}{slug}:")
    }

    /// Deletes every value whose key starts with `prefix`.
    pub fn forget(&self, prefix: &str) {
        let deleted = self.connection.execute(
            "DELETE FROM state WHERE substr(key, 1, length(?1)) = ?1",
            [prefix],
        );
        if let Err(error) = deleted {
            log::warn!("store: cannot forget {prefix}: {error}");
        }
    }
}
