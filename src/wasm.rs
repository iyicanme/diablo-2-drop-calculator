//! Raw C-ABI entry points for the browser build (`web/`). No wasm-bindgen: the page calls
//! `generate`, then copies each column straight out of linear memory via `column_ptr`/`column_len`.
//!
//! A full run is millions of rows, so instead of shipping `DropRow`s the result is columnar:
//! string fields become u16 indices into one shared dictionary (sent as a JSON array), the
//! terrorized/immunity booleans are packed into one bitmask byte, and the drop chance stays f64.

use crate::data::GameData;
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

#[derive(Default)]
struct Output {
    dictionary: Vec<String>,
    lookup: HashMap<String, u16>,
    overflowed: bool,
    strings: [Vec<u16>; STRING_COLUMNS],
    act: Vec<u8>,
    /// bit 0 terrorized, bits 1-6 fire/cold/lightning/poison/magic/physical immune.
    flags: Vec<u8>,
    category: Vec<u8>,
    chance: Vec<f64>,
    dictionary_json: String,
}

impl Output {
    fn intern(&mut self, s: &str) -> u16 {
        if let Some(&i) = self.lookup.get(s) {
            return i;
        }
        let Ok(i) = u16::try_from(self.dictionary.len()) else {
            self.overflowed = true;
            return 0;
        };
        self.dictionary.push(s.to_string());
        self.lookup.insert(s.to_string(), i);
        i
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

    output.lookup = HashMap::new();
    output.dictionary_json = serde_json::to_string(&output.dictionary).unwrap_or_default();
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
