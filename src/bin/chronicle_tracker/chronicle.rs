//! Reads the Chronicle out of a Reign of the Warlock shared stash (.d2i).
//!
//! The stash is a run of sectors, each starting with a 64-byte header (magic `55 AA 55 AA`,
//! sector length at offset 0x10). The Chronicle is the sector whose payload starts with
//! `C0 ED EA C0`. Its payload is:
//! - 20-byte header: magic, u16 version (1), u16 set count, u16 unique count, u16 runeword count,
//!   8 reserved bytes.
//! - One 10-byte record per entry, sets first, then uniques, then runewords: u32 id, u16 of
//!   unknown meaning, u32 minutes since the Unix epoch (UTC) when the item was recorded. Set and
//!   unique ids are their row ids; a runeword id is the string id of its name (see [`runeword_row`]).
//! - A 64-byte block of unknown meaning.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};

use crate::items::{self, Kind};

const SECTOR_MAGIC: [u8; 4] = [0x55, 0xAA, 0x55, 0xAA];
const SECTOR_HEADER_SIZE: usize = 0x40;
const CHRONICLE_MAGIC: [u8; 4] = [0xC0, 0xED, 0xEA, 0xC0];
const CHRONICLE_VERSION: u16 = 1;
const HEADER_SIZE: usize = 20;
const RECORD_SIZE: usize = 10;
/// English strings of runeword names, keyed `RunewordN` with their string ids.
const ITEM_RUNES_JSON: &str = include_str!("../../../data/item-runes.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: Kind,
    pub row: u32,
    /// Minutes since the Unix epoch (UTC).
    pub found_at: u32,
}

pub fn default_path(hardcore: bool) -> PathBuf {
    let mode = if hardcore { "HardCore" } else { "SoftCore" };
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Saved Games")
        .join("Diablo II Resurrected")
        .join(format!("ModernSharedStash{mode}V2.d2i"))
}

pub fn load(path: &Path) -> Result<Vec<Entry>> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    parse_stash(&bytes).with_context(|| format!("parsing {}", path.display()))
}

pub fn parse_stash(bytes: &[u8]) -> Result<Vec<Entry>> {
    let mut off = 0;
    while off < bytes.len() {
        let Some(header) = bytes.get(off..off + SECTOR_HEADER_SIZE) else {
            bail!("truncated stash sector at {off:#x}");
        };
        if header[..4] != SECTOR_MAGIC {
            bail!("bad stash sector magic at {off:#x}");
        }
        let len = u32_at(header, 0x10) as usize;
        if len < SECTOR_HEADER_SIZE || off + len > bytes.len() {
            bail!("bad stash sector length {len:#x} at {off:#x}");
        }
        let payload = &bytes[off + SECTOR_HEADER_SIZE..off + len];
        if payload.starts_with(&CHRONICLE_MAGIC) {
            return parse_chronicle(payload);
        }
        off += len;
    }
    bail!("no Chronicle in this stash (it needs a Reign of the Warlock shared stash)")
}

fn parse_chronicle(payload: &[u8]) -> Result<Vec<Entry>> {
    if payload.len() < HEADER_SIZE {
        bail!("truncated Chronicle header");
    }
    let version = u16_at(payload, 4);
    if version != CHRONICLE_VERSION {
        bail!("unsupported Chronicle version {version}");
    }
    let lists = [
        (Kind::Set, u16_at(payload, 6)),
        (Kind::Unique, u16_at(payload, 8)),
        (Kind::Runeword, u16_at(payload, 10)),
    ];
    let total: usize = lists.iter().map(|&(_, n)| usize::from(n)).sum();
    if HEADER_SIZE + total * RECORD_SIZE > payload.len() {
        bail!(
            "Chronicle claims {total} entries but is only {} bytes",
            payload.len()
        );
    }

    let mut records = payload[HEADER_SIZE..].as_chunks::<RECORD_SIZE>().0.iter();
    let mut entries = Vec::with_capacity(total);
    for (kind, count) in lists {
        for rec in records.by_ref().take(count.into()) {
            let id = u32_at(rec, 0);
            let row = if kind == Kind::Runeword {
                runeword_row(id)
            } else {
                id
            };
            entries.push(Entry {
                kind,
                row,
                found_at: u32_at(rec, 6),
            });
        }
    }
    Ok(entries)
}

/// Maps the string id a runeword record stores to the runeword's row. Most runewords' string
/// ids are `0x5000 + row`, but newer ones (Hysteria, Coven, ...) and Delirium's are not, so this
/// matches the string's English text against the runeword names. Unknown ids come back unchanged.
pub fn runeword_row(string_id: u32) -> u32 {
    static ROWS: LazyLock<HashMap<u32, u32>> = LazyLock::new(|| {
        #[derive(serde::Deserialize)]
        struct Str<'a> {
            id: u32,
            #[serde(rename = "Key", borrow)]
            key: &'a str,
            #[serde(rename = "enUS", borrow)]
            name: &'a str,
        }
        // D2R saves its string tables with a UTF-8 BOM, which serde_json rejects.
        let json = ITEM_RUNES_JSON.trim_start_matches('\u{feff}');
        let strings: Vec<Str> = serde_json::from_str(json).expect("bad data/item-runes.json");
        let rows: HashMap<&str, u32> = items::catalog()
            .iter()
            .filter(|i| i.kind == Kind::Runeword)
            .map(|i| (i.name, i.row))
            .collect();
        strings
            .iter()
            .filter(|s| s.key.starts_with("Runeword"))
            .filter_map(|s| Some((s.id, *rows.get(s.name)?)))
            .collect()
    });
    ROWS.get(&string_id).copied().unwrap_or(string_id)
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Formats minutes since the Unix epoch as a UTC `YYYY-MM-DD` date.
pub fn fmt_date(minutes: u32) -> String {
    // Howard Hinnant's days-to-civil algorithm.
    let z = i64::from(minutes) / (24 * 60) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Builds a stash with one empty item sector followed by a Chronicle sector.
    pub fn stash(sets: &[(u32, u32)], uniques: &[(u32, u32)], runewords: &[(u32, u32)]) -> Vec<u8> {
        let mut payload = CHRONICLE_MAGIC.to_vec();
        payload.extend(CHRONICLE_VERSION.to_le_bytes());
        for list in [sets, uniques, runewords] {
            payload.extend(u16::try_from(list.len()).unwrap().to_le_bytes());
        }
        payload.extend([0; 8]);
        for &(id, minutes) in sets.iter().chain(uniques).chain(runewords) {
            payload.extend(id.to_le_bytes());
            payload.extend([0xAB, 0xCD]);
            payload.extend(minutes.to_le_bytes());
        }
        payload.extend([0; 64]);

        let mut out = Vec::new();
        for body in [&b"JM\0\0"[..], &payload] {
            let mut header = [0u8; SECTOR_HEADER_SIZE];
            header[..4].copy_from_slice(&SECTOR_MAGIC);
            let len = u32::try_from(SECTOR_HEADER_SIZE + body.len()).unwrap();
            header[0x10..0x14].copy_from_slice(&len.to_le_bytes());
            out.extend(header);
            out.extend(body);
        }
        out
    }

    #[test]
    fn reads_entries_in_list_order() {
        let bytes = stash(&[(3, 60)], &[(0, 120), (419, 180)], &[(0x5099, 240)]);
        let entries = parse_stash(&bytes).unwrap();
        assert_eq!(
            entries,
            vec![
                Entry {
                    kind: Kind::Set,
                    row: 3,
                    found_at: 60
                },
                Entry {
                    kind: Kind::Unique,
                    row: 0,
                    found_at: 120
                },
                Entry {
                    kind: Kind::Unique,
                    row: 419,
                    found_at: 180
                },
                Entry {
                    kind: Kind::Runeword,
                    row: 0x99,
                    found_at: 240
                },
            ]
        );
    }

    #[test]
    fn maps_runeword_string_ids_to_rows() {
        // Smoke follows the usual 0x5000 + row pattern; Coven (a Reign of the Warlock runeword)
        // and Delirium don't.
        assert_eq!(runeword_row(0x5099), 153);
        assert_eq!(runeword_row(27650), 205);
        assert_eq!(runeword_row(10910), 48);
        let bytes = stash(&[], &[], &[(27650, 60)]);
        assert_eq!(parse_stash(&bytes).unwrap()[0].row, 205);
    }

    #[test]
    fn every_runeword_has_a_string_id() {
        let mut ids: HashMap<u32, u32> = HashMap::new();
        for id in 0..0x10000 {
            let row = runeword_row(id);
            if row != id {
                ids.insert(row, id);
            }
        }
        for item in crate::items::catalog()
            .iter()
            .filter(|i| i.kind == Kind::Runeword)
        {
            assert!(
                ids.contains_key(&item.row),
                "no string id for {}",
                item.name
            );
        }
    }

    #[test]
    fn rejects_stash_without_chronicle() {
        let mut bytes = stash(&[], &[], &[]);
        bytes.truncate(SECTOR_HEADER_SIZE + 4);
        assert!(
            parse_stash(&bytes)
                .unwrap_err()
                .to_string()
                .contains("no Chronicle")
        );
    }

    #[test]
    fn rejects_overlong_counts() {
        let mut bytes = stash(&[(1, 1)], &[], &[]);
        let counts = 2 * SECTOR_HEADER_SIZE + 4 + 6;
        bytes[counts] = 200;
        assert!(parse_stash(&bytes).is_err());
    }

    #[test]
    fn formats_dates() {
        assert_eq!(fmt_date(0), "1970-01-01");
        // 0x01c30b37 minutes, the first record of a real stash.
        assert_eq!(fmt_date(0x01C3_0B37), "2026-03-15");
        assert_eq!(fmt_date(29_808_000), "2026-09-04");
    }
}
