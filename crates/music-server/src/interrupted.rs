//! Songs asked for and not finished when the studio closed.
//!
//! The engine's queue lives in the engine process and the service's list of
//! jobs in memory, so closing the studio ends every song still being made, and
//! nothing remembered that it was asked for. Each request is written here when
//! the engine takes it and struck off when its job ends. What is left when the
//! studio opens again was interrupted: it is offered back to be made again,
//! never started on its own, since a song takes minutes of the whole card.

use std::{path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub style: String,
    pub lyrics: String,
    pub submitted_at: u64,
    /// Left over from an earlier run of the studio.
    pub interrupted: bool,
    /// The request as it was sent, to make the song again.
    pub request: Value,
}

pub struct Journal {
    path: Option<PathBuf>,
    entries: Mutex<Vec<Entry>>,
}

impl Journal {
    /// Reads what the last run left; all of it was interrupted.
    pub fn open(path: Option<PathBuf>) -> Self {
        let mut entries: Vec<Entry> = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        for entry in &mut entries {
            entry.interrupted = true;
        }
        let journal = Self { path, entries: Mutex::new(Vec::new()) };
        if !entries.is_empty() {
            journal.save(&entries);
        }
        *journal.lock() = entries;
        journal
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Entry>> {
        self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn save(&self, entries: &[Entry]) {
        let Some(path) = &self.path else { return };
        let written = serde_json::to_vec_pretty(entries).map_err(|error| error.to_string()).and_then(|bytes| {
            if let Some(folder) = path.parent() {
                std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
            }
            let temporary = path.with_extension("json.tmp");
            std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
            std::fs::rename(&temporary, path).map_err(|error| error.to_string())
        });
        if let Err(error) = written {
            eprintln!("[ERROR] the list of songs being made could not be saved: {error}");
        }
    }

    pub fn record(&self, entry: Entry) {
        let mut entries = self.lock();
        entries.retain(|existing| existing.id != entry.id);
        entries.push(entry);
        self.save(&entries);
    }

    /// Strikes a job off, finished or not; hands back what was kept.
    pub fn forget(&self, id: &str) -> Option<Entry> {
        let mut entries = self.lock();
        let place = entries.iter().position(|entry| entry.id == id)?;
        let removed = entries.remove(place);
        self.save(&entries);
        Some(removed)
    }

    pub fn interrupted(&self) -> Vec<Entry> {
        self.lock().iter().filter(|entry| entry.interrupted).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> Entry {
        Entry { id: id.into(), title: "t".into(), style: "s".into(), lyrics: "l".into(), submitted_at: 1, interrupted: false, request: serde_json::json!({ "style": "s" }) }
    }

    #[test]
    fn a_song_not_finished_when_the_studio_closed_is_offered_back_once() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("music-jobs.json");
        let first = Journal::open(Some(path.clone()));
        first.record(entry("done"));
        first.record(entry("cut-off"));
        first.forget("done");
        assert!(first.interrupted().is_empty(), "a song of this run is not interrupted");

        let second = Journal::open(Some(path.clone()));
        let offered = second.interrupted();
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].id, "cut-off");

        second.forget("cut-off");
        assert!(Journal::open(Some(path)).interrupted().is_empty());
    }

    #[test]
    fn without_a_file_nothing_is_remembered_and_nothing_fails() {
        let journal = Journal::open(None);
        journal.record(entry("a"));
        assert!(journal.forget("a").is_some());
        assert!(journal.forget("a").is_none());
    }
}
