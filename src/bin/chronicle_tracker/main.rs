mod app;
mod chronicle;
mod items;
mod ui;
mod watch;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;

use app::App;
use watch::StashWatcher;

/// Diablo 2 Resurrected Chronicle tracker. Lists the set items, uniques and runewords
/// your Reign of the Warlock shared stash hasn't recorded yet.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Shared stash to read (default: ModernSharedStash{SoftCore,HardCore}V2.d2i in Saved Games)
    #[arg(long, env = "CHRONICLE_TRACKER_FILE")]
    file: Option<PathBuf>,
    /// Read the hardcore stash instead of the softcore one
    #[arg(long)]
    hardcore: bool,
}

/// How long to wait for a key before checking the stash file again.
const TICK: Duration = Duration::from_millis(100);
/// How many newly found item names to list in the status line.
const NAMES_SHOWN: usize = 3;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = cli
        .file
        .unwrap_or_else(|| chronicle::default_path(cli.hardcore));
    let mut app = App::new(&chronicle::load(&path)?);
    ratatui::run(|terminal| run(terminal, &mut app, &path))
}

fn run(terminal: &mut DefaultTerminal, app: &mut App, path: &Path) -> Result<()> {
    let mut watcher = StashWatcher::new(path);
    app.watching = watcher.error.is_none();
    app.status = match &watcher.error {
        None => format!("Watching {}", path.display()),
        Some(e) => format!("File watcher unavailable ({e}), checking every second instead"),
    };
    while !app.quit {
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(TICK)? {
            // Windows also reports key releases; only act on presses.
            if let Some(key) = event::read()?.as_key_press_event() {
                app.handle_key(key);
            }
        }
        if watcher.poll(path) || std::mem::take(&mut app.reload) {
            let ok = reload(app, path);
            watcher.read_result(ok);
        }
    }
    Ok(())
}

fn reload(app: &mut App, path: &Path) -> bool {
    match chronicle::load(path) {
        Ok(entries) => {
            let new = app.set_entries(&entries);
            app.status = match new.len() {
                0 => "Stash reloaded, nothing new".into(),
                n => {
                    let mut names: Vec<&str> = new
                        .iter()
                        .take(NAMES_SHOWN)
                        .map(|&i| app.items[i].name)
                        .collect();
                    if n > NAMES_SHOWN {
                        names.push("…");
                    }
                    format!("★ {n} new in your Chronicle: {}", names.join(", "))
                }
            };
            true
        }
        // The game may be mid-write; keep the old data, the watcher retries shortly.
        Err(e) => {
            app.status = format!("Reload failed: {e:#}");
            false
        }
    }
}
