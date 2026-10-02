//! Raw C-ABI entry points for the browser build (`web/`). No wasm-bindgen: the page calls
//! `generate`, then copies each column straight out of linear memory via `column_ptr`/`column_len`.
//!
//! A full run is millions of rows, so instead of shipping `DropRow`s the result is columnar:
//! string fields become u16 indices into one shared dictionary (sent as a JSON array), the
//! terrorized/immunity booleans are packed into one bitmask byte, and the drop chance stays f64.
//! Hover-card references (monster / area / item) are u16 indices into a separate details JSON.

use crate::data::{DisplayData, GameData};
use crate::details;
use crate::export::{self, Category, DropRow};
use crate::pipeline;
use std::collections::HashMap;
use std::sync::Mutex;

/// Returned by `generate` for invalid input or when the dictionary outgrows u16 indices.
const ERROR: u32 = u32::MAX;

// Column order shared with web/worker.js.
const MONSTER: usize = 0;
const LOCATION: usize = 1;
const DIFFICULTY: usize = 2;
const ITEM: usize = 3;
const TREASURE_CLASS: usize = 4;
const ITEM_TYPE: usize = 5;
const TIER: usize = 6;
const QUALITY: usize = 7;
const MONSTER_TYPE: usize = 8;
const STRING_COLUMNS: usize = 9;
const ACT: usize = 9;
const FLAGS: usize = 10;
const CATEGORY: usize = 11;
const CHANCE: usize = 12;
const DICTIONARY: usize = 13;
const MONSTER_REF: usize = 14;
const AREA_REF: usize = 15;
const ITEM_REF: usize = 16;
const LEVEL: usize = 17;
const DETAILS: usize = 18;

/// Distinct keys in first-seen order, with u16 indices.
#[derive(Default)]
struct Interner {
    keys: Vec<String>,
    lookup: HashMap<String, u16>,
}

impl Interner {
    /// `None` once there are more than u16::MAX distinct keys.
    fn intern(&mut self, s: &str) -> Option<u16> {
        if let Some(&i) = self.lookup.get(s) {
            return Some(i);
        }
        let i = u16::try_from(self.keys.len()).ok()?;
        self.keys.push(s.to_string());
        self.lookup.insert(s.to_string(), i);
        Some(i)
    }
}

#[derive(Default)]
struct Output {
    dictionary: Interner,
    overflowed: bool,
    strings: [Vec<u16>; STRING_COLUMNS],
    act: Vec<u8>,
    /// bit 0 terrorized, bits 1-6 fire/cold/lightning/poison/magic/physical immune.
    flags: Vec<u8>,
    category: Vec<u8>,
    chance: Vec<f64>,
    dictionary_json: String,
    /// Monster keys, area ids and item keys referenced by the rows (see `DropRow`).
    refs: [Interner; 3],
    ref_columns: [Vec<u16>; 3],
    level: Vec<u8>,
    details_json: String,
}

impl Output {
    fn intern(&mut self, s: &str) -> u16 {
        self.dictionary.intern(s).unwrap_or_else(|| {
            self.overflowed = true;
            0
        })
    }

    fn push(&mut self, row: &DropRow) {
        let fields: [&str; STRING_COLUMNS] = [
            &row.monster_name,
            &row.monster_location,
            row.difficulty,
            &row.item_name,
            &row.item_tc,
            &row.item_type,
            row.item_tier,
            row.magic_quality,
            row.monster_type,
        ];
        for (column, field) in fields.into_iter().enumerate() {
            let i = self.intern(field);
            self.strings[column].push(i);
        }
        let bits = [
            row.terrorized,
            row.res_fire,
            row.res_cold,
            row.res_lightning,
            row.res_poison,
            row.res_magic,
            row.res_physical,
        ];
        self.flags.push(bits.iter().enumerate().fold(0, |acc, (bit, &set)| acc | ((set as u8) << bit)));
        self.act.push(row.act as u8);
        self.category.push(Category::of(row) as u8);
        self.chance.push(row.drop_chance);
        for (i, key) in [&row.monster_key, &row.area_id, &row.item_key].into_iter().enumerate() {
            let index = self.refs[i].intern(key).unwrap_or_else(|| {
                self.overflowed = true;
                0
            });
            self.ref_columns[i].push(index);
        }
        self.level.push(row.monster_level.clamp(0, 255) as u8);
    }

    fn column_bytes(&self, column: usize) -> &[u8] {
        fn bytes<T>(v: &[T]) -> &[u8] {
            // SAFETY: plain-old-data numeric slices, viewed as their underlying bytes.
            unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), std::mem::size_of_val(v)) }
        }
        match column {
            c if c < STRING_COLUMNS => bytes(&self.strings[c]),
            ACT => &self.act,
            FLAGS => &self.flags,
            CATEGORY => &self.category,
            CHANCE => bytes(&self.chance),
            DICTIONARY => self.dictionary_json.as_bytes(),
            MONSTER_REF => bytes(&self.ref_columns[0]),
            AREA_REF => bytes(&self.ref_columns[1]),
            ITEM_REF => bytes(&self.ref_columns[2]),
            LEVEL => &self.level,
            DETAILS => self.details_json.as_bytes(),
            _ => &[],
        }
    }
}

static RESULT: Mutex<Option<Output>> = Mutex::new(None);

/// Runs the same computation as the CLI. `max_chance_per_x <= 0` means "keep everything".
/// Returns the row count, or `ERROR`.
#[unsafe(no_mangle)]
pub extern "C" fn generate(players: i32, magic_find: i32, character_level: i32, max_chance_per_x: f64) -> u32 {
    free_result();
    if !(1..=8).contains(&players) || magic_find < 0 {
        return ERROR;
    }
    let max_chance_per_x = (max_chance_per_x > 0.0).then_some(max_chance_per_x);

    let game_data = GameData::load();
    let config = pipeline::Config { players, magic_find: magic_find as i64, character_level };

    let mut output = Output::default();
    pipeline::generate_with(&game_data, &config, |row| {
        if let Some(max) = max_chance_per_x {
            if export::chance_per_x(&row).is_none_or(|x| x > max) {
                return;
            }
        }
        output.push(&row);
    });
    if output.overflowed {
        return ERROR;
    }

    output.dictionary_json = serde_json::to_string(&output.dictionary.keys).unwrap_or_default();
    output.dictionary.lookup = HashMap::new();
    let [monsters, areas, items] = &output.refs;
    let details = details::build(&game_data, &DisplayData::load(), &monsters.keys, &areas.keys, &items.keys);
    output.details_json = details.to_string();
    output.refs = Default::default();
    let rows = output.chance.len() as u32;
    *RESULT.lock().unwrap() = Some(output);
    rows
}

#[unsafe(no_mangle)]
pub extern "C" fn column_ptr(column: u32) -> *const u8 {
    RESULT.lock().unwrap().as_ref().map_or(std::ptr::null(), |o| o.column_bytes(column as usize).as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn column_len(column: u32) -> u32 {
    RESULT.lock().unwrap().as_ref().map_or(0, |o| o.column_bytes(column as usize).len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn free_result() {
    *RESULT.lock().unwrap() = None;
}
