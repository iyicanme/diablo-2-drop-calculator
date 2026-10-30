//! Notices when the game rewrites the stash file.
//!
//! A file-system watcher on the stash's folder gives near-instant updates (the folder, not the
//! file, so it keeps working when the game replaces the file instead of writing in place). A
//! once-a-second check of the file's size and modified time backs it up in case the watcher
//! misses something or can't start.

use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Wait this long after the last change before reading, so we don't catch the game mid-write.
const SETTLE: Duration = Duration::from_millis(300);
/// How often the fallback compares the file's size and modified time.
const FALLBACK_EVERY: Duration = Duration::from_secs(1);
/// Reads that fail (e.g. on a half-written file) are retried this many times.
const MAX_RETRIES: u32 = 3;

type Stamp = Option<(SystemTime, u64)>;

pub struct StashWatcher {
    /// Kept alive for as long as we want events; `None` if it couldn't start.
    _watcher: Option<RecommendedWatcher>,
    events: Receiver<()>,
    stamp: Stamp,
    last_check: Instant,
    /// When the most recent unhandled change was seen.
    pending: Option<Instant>,
    retries: u32,
    /// Why the watcher couldn't start, if it didn't.
    pub error: Option<String>,
}

impl StashWatcher {
    pub fn new(path: &Path) -> Self {
        let (tx, events) = mpsc::channel();
        let (watcher, error) = match start(path, tx) {
            Ok(w) => (Some(w), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        Self {
            _watcher: watcher,
            events,
            stamp: stamp(path),
            last_check: Instant::now(),
            pending: None,
            retries: 0,
            error,
        }
    }

    /// True once the file has changed and settled, meaning it's time to re-read it.
    pub fn poll(&mut self, path: &Path) -> bool {
        let now = Instant::now();
        if self.events.try_iter().count() > 0 {
            self.pending = Some(now);
        }
        if now.duration_since(self.last_check) >= FALLBACK_EVERY {
            self.last_check = now;
            let s = stamp(path);
            if s != self.stamp {
                self.stamp = s;
                self.pending.get_or_insert(now);
            }
        }
        match self.pending {
            Some(t) if now.duration_since(t) >= SETTLE => {
                self.pending = None;
                self.stamp = stamp(path);
                true
            }
            _ => false,
        }
    }

    /// Reports how the last read went; a failed read is tried again after it settles.
    pub fn read_result(&mut self, ok: bool) {
        if ok {
            self.retries = 0;
        } else if self.retries < MAX_RETRIES {
            self.retries += 1;
            self.pending = Some(Instant::now());
        }
    }
}

fn start(path: &Path, tx: mpsc::Sender<()>) -> Result<RecommendedWatcher> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .context("stash path has no file name")?
        .to_owned();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        // Windows file names are case-insensitive.
        let ours = event.paths.iter().any(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy()
                    .eq_ignore_ascii_case(&name.to_string_lossy())
            })
        });
        if ours {
            let _ = tx.send(());
        }
    })
    .context("starting file watcher")?;
    watcher
        .watch(dir, RecursiveMode::NonRecursive)
        .with_context(|| format!("watching {}", dir.display()))?;
    Ok(watcher)
}

fn stamp(path: &Path) -> Stamp {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for_change(w: &mut StashWatcher, path: &Path) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if w.poll(path) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn sees_writes_and_replacements() {
        let dir = std::env::temp_dir().join(format!("chronicle_watch_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Stash.d2i");
        std::fs::write(&path, b"one").unwrap();

        let mut w = StashWatcher::new(&path);
        assert!(w.error.is_none(), "{:?}", w.error);
        assert!(!w.poll(&path));

        std::fs::write(&path, b"two!").unwrap();
        assert!(wait_for_change(&mut w, &path), "missed an in-place write");

        // Write-then-rename, the way many programs save.
        let tmp = dir.join("Stash.tmp");
        std::fs::write(&tmp, b"three").unwrap();
        std::fs::rename(&tmp, &path).unwrap();
        assert!(wait_for_change(&mut w, &path), "missed a replacement");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
