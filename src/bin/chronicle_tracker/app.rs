//! TUI state and key handling (no terminal I/O, so it's unit-testable).

use std::collections::HashMap;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::chronicle::Entry;
use crate::items::{self, Item, Kind, SetBonuses, Tier};

const PAGE: isize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
}

/// Tab 0 shows every kind; tabs 1..=3 show `Kind::ALL[tab - 1]`.
pub const TAB_COUNT: usize = 1 + Kind::ALL.len();

pub struct App {
    pub items: Vec<Item>,
    /// Partial and full set bonuses by set name.
    pub sets: HashMap<&'static str, SetBonuses>,
    /// When each item (same index as `items`) was first recorded, in minutes since the epoch.
    pub found: Vec<Option<u32>>,
    /// Chronicle entries that match no known item.
    pub unknown: Vec<Entry>,
    pub tab: usize,
    pub show_found: bool,
    /// Only show items of this base tier (hiding items that have no tier).
    pub tier: Option<Tier>,
    pub search: String,
    pub mode: Mode,
    /// Index of the selected row in `visible()`.
    pub row: usize,
    /// How many lines the details pane is scrolled down (reset when the selection changes).
    pub detail_scroll: u16,
    pub status: String,
    pub table_state: TableState,
    pub quit: bool,
    /// Set when the user asked to re-read the stash file.
    pub reload: bool,
    /// Whether the file-system watcher is running (otherwise only the fallback check is).
    pub watching: bool,
}

impl App {
    pub fn new(entries: &[Entry]) -> Self {
        let items = items::catalog();
        let mut app = Self {
            found: vec![None; items.len()],
            items,
            sets: items::set_bonuses(),
            unknown: Vec::new(),
            tab: 0,
            show_found: false,
            tier: None,
            search: String::new(),
            mode: Mode::Normal,
            row: 0,
            detail_scroll: 0,
            status: String::new(),
            table_state: TableState::default(),
            quit: false,
            reload: false,
            watching: false,
        };
        app.set_entries(entries);
        app
    }

    /// Replaces the Chronicle contents, keeping the view where it was. Returns the indices of
    /// items that weren't found before.
    pub fn set_entries(&mut self, entries: &[Entry]) -> Vec<usize> {
        let before = std::mem::replace(&mut self.found, vec![None; self.items.len()]);
        let by_row: HashMap<(Kind, u32), usize> = self
            .items
            .iter()
            .enumerate()
            .flat_map(|(i, item)| item.rows.iter().map(move |&r| ((item.kind, r), i)))
            .collect();
        self.unknown.clear();
        for e in entries {
            match by_row.get(&(e.kind, e.row)) {
                Some(&i) => {
                    let f = &mut self.found[i];
                    *f = Some(f.map_or(e.found_at, |t| t.min(e.found_at)));
                }
                None => self.unknown.push(*e),
            }
        }
        self.clamp_row();
        (0..self.items.len())
            .filter(|&i| before[i].is_none() && self.found[i].is_some())
            .collect()
    }

    pub fn tab_kind(&self) -> Option<Kind> {
        self.tab.checked_sub(1).map(|i| Kind::ALL[i])
    }

    /// (found, total) for one kind, or for everything.
    pub fn progress(&self, kind: Option<Kind>) -> (usize, usize) {
        let mut done = 0;
        let mut total = 0;
        for (item, found) in self.items.iter().zip(&self.found) {
            if kind.is_none_or(|k| item.kind == k) {
                total += 1;
                done += usize::from(found.is_some());
            }
        }
        (done, total)
    }

    /// How many items of a kind (or of every kind) are left, counting only the selected tier.
    pub fn remaining(&self, kind: Option<Kind>) -> usize {
        (0..self.items.len())
            .filter(|&i| {
                let item = &self.items[i];
                kind.is_none_or(|k| item.kind == k)
                    && self.tier_matches(item)
                    && self.found[i].is_none()
            })
            .count()
    }

    fn tier_matches(&self, item: &Item) -> bool {
        self.tier.is_none_or(|t| item.tier == Some(t))
    }

    /// Item indices shown in the table.
    pub fn visible(&self) -> Vec<usize> {
        let kind = self.tab_kind();
        let needle = self.search.to_lowercase();
        (0..self.items.len())
            .filter(|&i| {
                let item = &self.items[i];
                kind.is_none_or(|k| item.kind == k)
                    && self.tier_matches(item)
                    && (self.show_found || self.found[i].is_none())
                    && (needle.is_empty()
                        || [item.name, item.base, item.set, item.item_type, item.runes]
                            .iter()
                            .any(|s| s.to_lowercase().contains(&needle)))
            })
            .collect()
    }

    pub fn selected(&self) -> Option<usize> {
        self.visible().get(self.row).copied()
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        // Some terminals (e.g. on Windows) report Shift+j as 'j' with SHIFT rather than as 'J'.
        let code = match key.code {
            KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::SHIFT) => {
                KeyCode::Char(c.to_ascii_uppercase())
            }
            code => code,
        };
        let before = self.selected();
        match self.mode {
            Mode::Normal => self.handle_normal(code),
            Mode::Search => self.handle_search(code),
        }
        if self.selected() != before {
            self.detail_scroll = 0;
        }
    }

    fn handle_normal(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up => self.move_by(-1),
            KeyCode::Down => self.move_by(1),
            KeyCode::PageUp => self.move_by(-PAGE),
            KeyCode::PageDown => self.move_by(PAGE),
            KeyCode::Home => self.move_by(isize::MIN),
            KeyCode::End => self.move_by(isize::MAX),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.set_tab((self.tab + 1) % TAB_COUNT)
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.set_tab((self.tab + TAB_COUNT - 1) % TAB_COUNT);
            }
            KeyCode::Char(c @ '1'..='4') => self.set_tab(c as usize - '1' as usize),
            KeyCode::Char('f') => {
                self.show_found = !self.show_found;
                self.status = if self.show_found {
                    "Showing found items too"
                } else {
                    "Showing remaining items only"
                }
                .into();
                self.row = 0;
            }
            // The details pane clamps this to its last line when drawn (ui::draw_details).
            KeyCode::Char('j' | 'J') => self.detail_scroll = self.detail_scroll.saturating_add(1),
            KeyCode::Char('k' | 'K') => self.detail_scroll = self.detail_scroll.saturating_sub(1),
            KeyCode::Char('t') => self.cycle_tier(),
            KeyCode::Char('/') => self.mode = Mode::Search,
            KeyCode::Esc if !self.search.is_empty() => {
                self.search.clear();
                self.clamp_row();
            }
            KeyCode::Char('r') => self.reload = true,
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }

    fn handle_search(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter => self.mode = Mode::Normal,
            KeyCode::Esc => {
                self.search.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.search.pop();
            }
            KeyCode::Char(c) => self.search.push(c),
            _ => return,
        }
        self.row = 0;
    }

    /// All tiers, then Normal, Exceptional, Elite, and back to all.
    fn cycle_tier(&mut self) {
        self.tier = match self.tier {
            None => Some(Tier::Normal),
            Some(Tier::Normal) => Some(Tier::Exceptional),
            Some(Tier::Exceptional) => Some(Tier::Elite),
            Some(Tier::Elite) => None,
        };
        self.status = match self.tier {
            Some(t) => format!("Showing {} items only", t.label()),
            None => "Showing all tiers".into(),
        };
        self.row = 0;
    }

    fn set_tab(&mut self, tab: usize) {
        self.tab = tab;
        self.row = 0;
    }

    fn move_by(&mut self, delta: isize) {
        self.row = self.row.saturating_add_signed(delta);
        self.clamp_row();
    }

    fn clamp_row(&mut self) {
        self.row = self.row.min(self.visible().len().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(app: &mut App, codes: &[KeyCode]) {
        for &c in codes {
            app.handle_key(KeyEvent::from(c));
        }
    }

    fn chars(s: &str) -> Vec<KeyCode> {
        s.chars().map(KeyCode::Char).collect()
    }

    fn row_of(app: &App, name: &str) -> (Kind, u32) {
        let item = app.items.iter().find(|i| i.name == name).unwrap();
        (item.kind, item.rows[0])
    }

    fn entry((kind, row): (Kind, u32), found_at: u32) -> Entry {
        Entry {
            kind,
            row,
            found_at,
        }
    }

    #[test]
    fn found_items_leave_the_remaining_list() {
        let blank = App::new(&[]);
        let shako = entry(row_of(&blank, "Harlequin Crest"), 100);
        let spirit = entry(row_of(&blank, "Spirit"), 200);
        let mut app = App::new(&[
            shako,
            spirit,
            Entry {
                kind: Kind::Set,
                row: 999,
                found_at: 1,
            },
        ]);

        assert_eq!(app.progress(None), (2, 649));
        assert_eq!(app.progress(Some(Kind::Runeword)), (1, 99));
        assert_eq!(app.unknown.len(), 1);
        let names = |app: &App| {
            app.visible()
                .iter()
                .map(|&i| app.items[i].name)
                .collect::<Vec<_>>()
        };
        assert!(!names(&app).contains(&"Harlequin Crest"));

        press(&mut app, &chars("f"));
        assert!(names(&app).contains(&"Harlequin Crest"));
    }

    #[test]
    fn reload_reports_only_newly_found_items() {
        let mut app = App::new(&[]);
        let shako = entry(row_of(&app, "Harlequin Crest"), 100);
        let spirit = entry(row_of(&app, "Spirit"), 200);
        assert_eq!(app.set_entries(&[shako]).len(), 1);
        let new = app.set_entries(&[shako, spirit]);
        assert_eq!(
            new.iter().map(|&i| app.items[i].name).collect::<Vec<_>>(),
            ["Spirit"]
        );
    }

    #[test]
    fn tier_filter_cycles_and_hides_untiered_items() {
        let mut app = App::new(&[]);
        press(&mut app, &chars("t"));
        assert_eq!(app.tier, Some(Tier::Normal));
        press(&mut app, &chars("tt"));
        assert_eq!(app.tier, Some(Tier::Elite));
        let visible = app.visible();
        assert!(!visible.is_empty());
        assert!(
            visible
                .iter()
                .all(|&i| app.items[i].tier == Some(Tier::Elite))
        );
        assert_eq!(app.remaining(Some(Kind::Runeword)), 0);
        press(&mut app, &chars("t"));
        assert_eq!(app.tier, None);
        assert_eq!(app.remaining(None), 649);
    }

    #[test]
    fn search_matches_runes_and_type() {
        let mut app = App::new(&[]);
        press(&mut app, &chars("/jah ith ber"));
        let names: Vec<_> = app.visible().iter().map(|&i| app.items[i].name).collect();
        assert_eq!(names, ["Enigma"]);
    }

    #[test]
    fn duplicate_rows_count_once_with_earliest_date() {
        let blank = App::new(&[]);
        let item = blank
            .items
            .iter()
            .position(|i| i.name == "Azurewrath")
            .unwrap();
        let rows = &blank.items[item].rows;
        let app = App::new(&[
            Entry {
                kind: Kind::Unique,
                row: rows[1],
                found_at: 50,
            },
            Entry {
                kind: Kind::Unique,
                row: rows[0],
                found_at: 90,
            },
        ]);
        assert_eq!(app.found[item], Some(50));
        assert_eq!(app.progress(Some(Kind::Unique)), (1, 415));
    }

    #[test]
    fn details_scroll_resets_on_a_new_selection() {
        let mut app = App::new(&[]);
        press(&mut app, &chars("JJJK"));
        assert_eq!(app.detail_scroll, 2);
        press(&mut app, &[KeyCode::Down]);
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn j_and_k_scroll_details_not_the_list() {
        let mut app = App::new(&[]);
        press(&mut app, &chars("jjJ"));
        assert_eq!((app.row, app.detail_scroll), (0, 3));
        // Shift+k as some terminals report it.
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::SHIFT));
        press(&mut app, &chars("k"));
        assert_eq!((app.row, app.detail_scroll), (0, 1));
    }

    #[test]
    fn search_and_tabs_filter_the_list() {
        let mut app = App::new(&[]);
        press(&mut app, &chars("/tal rasha"));
        press(&mut app, &[KeyCode::Enter]);
        assert_eq!(app.visible().len(), 5);
        assert_eq!(app.mode, Mode::Normal);

        press(&mut app, &chars("3"));
        assert!(app.visible().is_empty());
        press(&mut app, &[KeyCode::Esc]);
        assert_eq!(app.visible().len(), 415);
        assert!(!app.quit);
        press(&mut app, &[KeyCode::End, KeyCode::Down]);
        assert_eq!(app.row, 414);
    }
}
