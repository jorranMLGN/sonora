use anyhow::{Context as _, Result};
use gpui::{Context, Task};
use rusqlite::{OptionalExtension as _, params};
use storage::Database;

use crate::{Io, join};

const KEEP: usize = 20;
const KEY: &str = "search-history";

/// The queries the user searched for, newest first, kept in the `app_state` table so they
/// survive a restart. Only finished searches are recorded, never every keystroke.
pub struct SearchHistory {
    entries: Vec<String>,
    loaded: bool,
    store: Store,
    io: Io,
    _load: Task<()>,
    save: Option<Task<()>>,
}

impl SearchHistory {
    pub fn new(database: Database, io: Io, cx: &mut Context<Self>) -> Self {
        let store = Store::new(database);
        let reading = store.clone();
        let loading =
            io.spawn(async move { tokio::task::spawn_blocking(move || reading.load()).await? });
        let load = cx.spawn(async move |this, cx| {
            let stored = join(loading).await.unwrap_or_else(|error| {
                log::warn!("search: history: {error:#}");
                Vec::new()
            });
            this.update(cx, |this, cx| {
                let fresh = std::mem::take(&mut this.entries);
                let changed = !fresh.is_empty();
                this.entries = merged(stored, fresh);
                this.loaded = true;
                if changed {
                    this.persist(cx);
                }
                cx.notify();
            })
            .ok();
        });

        Self {
            entries: Vec::new(),
            loaded: false,
            store,
            io,
            _load: load,
            save: None,
        }
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Puts a finished search at the front.
    pub fn record(&mut self, query: &str, cx: &mut Context<Self>) {
        if remembered(&mut self.entries, query) {
            self.persist(cx);
        }
    }

    pub fn remove(&mut self, query: &str, cx: &mut Context<Self>) {
        let folded = query.to_lowercase();
        self.entries.retain(|entry| entry.to_lowercase() != folded);
        self.persist(cx);
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.entries.clear();
        self.persist(cx);
    }

    /// Writes the list once the stored one is in, so a partial list never replaces it.
    fn persist(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        if !self.loaded {
            return;
        }
        let store = self.store.clone();
        let entries = self.entries.clone();
        let saving = self
            .io
            .spawn(async move { tokio::task::spawn_blocking(move || store.save(&entries)).await? });
        self.save = Some(cx.spawn(async move |_, _| {
            if let Err(error) = join(saving).await {
                log::warn!("search: history: {error:#}");
            }
        }));
    }
}

/// The `search-history` row of the `app_state` table: the list as one JSON array.
#[derive(Clone)]
struct Store {
    database: Database,
}

impl Store {
    fn new(database: Database) -> Self {
        Self { database }
    }

    fn load(&self) -> Result<Vec<String>> {
        let encoded: Option<String> = self
            .database
            .open()?
            .query_row(
                "SELECT value FROM app_state WHERE key = ?",
                params![KEY],
                |row| row.get(0),
            )
            .optional()
            .context("cannot read search history")?;
        encoded
            .map(|encoded| serde_json::from_str(&encoded).context("cannot decode search history"))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    fn save(&self, entries: &[String]) -> Result<()> {
        let encoded = serde_json::to_string(entries).context("cannot encode search history")?;
        self.database
            .open()?
            .execute(
                "INSERT INTO app_state (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![KEY, encoded],
            )
            .context("cannot save search history")?;
        Ok(())
    }
}

/// Puts `query` at the front of `entries`, dropping an earlier spelling of it and anything
/// past `KEEP`. Returns false for a blank query, which is never kept.
fn remembered(entries: &mut Vec<String>, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return false;
    }
    let folded = query.to_lowercase();
    entries.retain(|entry| entry.to_lowercase() != folded);
    entries.insert(0, query.to_owned());
    entries.truncate(KEEP);
    true
}

/// The stored list with the searches made while it loaded in front of it.
fn merged(stored: Vec<String>, fresh: Vec<String>) -> Vec<String> {
    let mut entries = stored;
    for query in fresh.iter().rev() {
        remembered(&mut entries, query);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(queries: &[&str]) -> Vec<String> {
        queries.iter().map(|query| (*query).to_owned()).collect()
    }

    #[test]
    fn a_record_goes_to_the_front_once() {
        let mut kept = entries(&["daft punk", "Queen"]);
        assert!(remembered(&mut kept, "queen"));
        assert_eq!(kept, entries(&["queen", "daft punk"]));
    }

    #[test]
    fn a_blank_query_is_not_kept() {
        let mut kept = entries(&["queen"]);
        assert!(!remembered(&mut kept, "   "));
        assert_eq!(kept, entries(&["queen"]));
    }

    #[test]
    fn only_the_newest_are_kept() {
        let mut kept = Vec::new();
        for at in 0..25 {
            remembered(&mut kept, &format!("query {at}"));
        }
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept[0], "query 24");
    }

    #[test]
    fn a_record_during_load_merges() {
        let merged = merged(entries(&["old", "queen"]), entries(&["Queen", "new"]));
        assert_eq!(merged, entries(&["Queen", "new", "old"]));
    }

    #[test]
    fn history_round_trips_through_sqlite() {
        let path = std::env::temp_dir().join(format!(
            "sonora-search-history-{}/state.sqlite",
            std::process::id()
        ));
        let store = Store::new(Database::at(path));
        store
            .save(&entries(&["queen", "abba"]))
            .expect("history saves");
        assert_eq!(
            store.load().expect("history loads"),
            entries(&["queen", "abba"])
        );
    }
}
