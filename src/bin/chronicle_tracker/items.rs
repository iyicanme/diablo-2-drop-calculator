//! The items the Chronicle tracks, embedded from `data/chronicle_items.tsv`, with their stats from
//! `data/item_details.tsv`.

use std::collections::HashMap;

const TSV: &str = include_str!("../../../data/chronicle_items.tsv");
const DETAILS_TSV: &str = include_str!("../../../data/item_details.tsv");

/// Runes in Horadric Cube order; a rune's number is its index here plus one (El = 1, Ber = 30).
pub const RUNES: [&str; 33] = [
    "El", "Eld", "Tir", "Nef", "Eth", "Ith", "Tal", "Ral", "Ort", "Thul", "Amn", "Sol", "Shael",
    "Dol", "Hel", "Io", "Lum", "Ko", "Fal", "Lem", "Pul", "Um", "Mal", "Ist", "Gul", "Vex", "Ohm",
    "Lo", "Sur", "Ber", "Jah", "Cham", "Zod",
];

/// Number of a rune (El = 1 ... Zod = 33).
fn rune_number(name: &str) -> Option<u8> {
    let i = RUNES.iter().position(|r| r.eq_ignore_ascii_case(name))?;
    u8::try_from(i + 1).ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Set,
    Unique,
    Runeword,
}

impl Kind {
    pub const ALL: [Self; 3] = [Self::Set, Self::Unique, Self::Runeword];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Set => "Sets",
            Self::Unique => "Uniques",
            Self::Runeword => "Runewords",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "set" => Some(Self::Set),
            "unique" => Some(Self::Unique),
            "runeword" => Some(Self::Runeword),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Normal,
    Exceptional,
    Elite,
}

impl Tier {
    pub const ALL: [Self; 3] = [Self::Normal, Self::Exceptional, Self::Elite];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Exceptional => "Exceptional",
            Self::Elite => "Elite",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.label() == s)
    }
}

#[derive(Debug)]
pub struct Item {
    pub kind: Kind,
    /// Chronicle row ids of this item (a few items have several rows).
    pub rows: Vec<u32>,
    pub name: &'static str,
    /// Base item name (empty for runewords).
    pub base: &'static str,
    /// Set name (empty unless `kind` is `Set`).
    pub set: &'static str,
    /// Base item tier; `None` for jewelry, charms, jewels and runewords.
    pub tier: Option<Tier>,
    /// Item type, e.g. "Bow" or "Body Armor".
    pub item_type: &'static str,
    /// Rune recipe in socketing order, e.g. "Tal Thul Ort Amn" (empty unless a runeword).
    pub runes: &'static str,
    /// Quality level (the `lvl` column) of a set or unique item; empty for runewords.
    pub qlvl: &'static str,
    /// Number of the highest rune in a runeword's recipe (El = 1 ... Zod = 33).
    pub top_rune: Option<u8>,
    /// `top_rune` with its name, e.g. "30 Ber" (empty unless a runeword).
    pub top_rune_label: String,
    pub details: Details,
}

/// What the game's tooltip shows for an item, with rolled stats as ranges like "+[10-15]".
#[derive(Debug, Default)]
pub struct Details {
    pub req_level: &'static str,
    /// Stat lines, in the order the game lists them.
    pub stats: Vec<&'static str>,
    /// Bonuses a set item gains as more of its set is worn, e.g. "(2 items) +10% Faster Cast Rate".
    pub set_bonuses: Vec<&'static str>,
    /// Socket count of a runeword (empty otherwise).
    pub sockets: &'static str,
    /// Item types a runeword can be made in, e.g. "Body Armor" (empty unless a runeword).
    pub bases: &'static str,
}

/// Bonuses for wearing several pieces of a set.
#[derive(Debug, Default)]
pub struct SetBonuses {
    /// e.g. "(2 items) Replenish Life +10".
    pub partial: Vec<&'static str>,
    pub full: Vec<&'static str>,
}

/// Fields of `data/item_details.tsv`: (kind, key, field, value).
fn detail_records() -> impl Iterator<Item = (&'static str, &'static str, &'static str, &'static str)>
{
    DETAILS_TSV
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let mut f = l.splitn(4, '\t');
            let mut next = || f.next().unwrap_or_else(|| panic!("short line {l:?}"));
            (next(), next(), next(), next())
        })
}

/// Item details by (kind, Chronicle row).
fn details() -> HashMap<(Kind, u32), Details> {
    let mut map: HashMap<(Kind, u32), Details> = HashMap::new();
    for (kind, key, field, value) in detail_records() {
        let Some(kind) = Kind::parse(kind) else {
            continue;
        };
        let row = key.parse().unwrap_or_else(|_| panic!("bad row {key:?}"));
        let d = map.entry((kind, row)).or_default();
        match field {
            "req" => d.req_level = value,
            "stat" => d.stats.push(value),
            "bonus" => d.set_bonuses.push(value),
            "sockets" => d.sockets = value,
            "bases" => d.bases = value,
            _ => panic!("bad field {field:?}"),
        }
    }
    map
}

/// Partial and full set bonuses by set name.
pub fn set_bonuses() -> HashMap<&'static str, SetBonuses> {
    let mut map: HashMap<&str, SetBonuses> = HashMap::new();
    for (_, set, field, value) in detail_records().filter(|r| r.0 == "setbonus") {
        let b = map.entry(set).or_default();
        match field {
            "partial" => b.partial.push(value),
            "full" => b.full.push(value),
            _ => panic!("bad set bonus field {field:?}"),
        }
    }
    map
}

impl Item {
    /// The base item, or the rune recipe for a runeword.
    pub const fn base_or_runes(&self) -> &'static str {
        if self.base.is_empty() {
            self.runes
        } else {
            self.base
        }
    }

    pub fn tier_label(&self) -> &'static str {
        self.tier.map_or("", Tier::label)
    }

    /// Sort key within a kind: quality level for sets and uniques, highest rune for runewords.
    fn level(&self) -> u16 {
        self.top_rune
            .map_or_else(|| self.qlvl.parse().unwrap_or(0), u16::from)
    }
}

/// All tracked items: sets, then uniques, then runewords; each sorted by quality level (highest
/// rune for runewords), then name.
pub fn catalog() -> Vec<Item> {
    let mut details = details();
    let mut items: Vec<Item> = TSV
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let mut f = l.split('\t');
            let mut next = || f.next().unwrap_or_default();
            let kind = Kind::parse(next()).unwrap_or_else(|| panic!("bad kind in line {l:?}"));
            let rows: Vec<u32> = next()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().unwrap())
                .collect();
            // Items with several rows use the last one, the current version (Azurewrath's first
            // row is an old, unspawnable one).
            let details = rows
                .iter()
                .rev()
                .find_map(|&r| details.remove(&(kind, r)))
                .unwrap_or_default();
            let (name, base, set) = (next(), next(), next());
            let tier = next();
            let tier = (!tier.is_empty())
                .then(|| Tier::parse(tier).unwrap_or_else(|| panic!("bad tier in line {l:?}")));
            let (item_type, runes, qlvl) = (next(), next(), next());
            let top_rune = runes
                .split(' ')
                .filter(|r| !r.is_empty())
                .map(|r| rune_number(r).unwrap_or_else(|| panic!("bad rune {r:?} in line {l:?}")))
                .max();
            let top_rune_label = top_rune.map_or(String::new(), |n| {
                format!("{n} {}", RUNES[usize::from(n) - 1])
            });
            Item {
                kind,
                rows,
                name,
                base,
                set,
                tier,
                item_type,
                runes,
                qlvl,
                top_rune,
                top_rune_label,
                details,
            }
        })
        .collect();
    items.sort_by_key(|i| (i.kind, i.level(), i.name));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_every_item_once() {
        let items = catalog();
        let count = |k| items.iter().filter(|i| i.kind == k).count();
        assert_eq!(
            (count(Kind::Set), count(Kind::Unique), count(Kind::Runeword)),
            (135, 415, 99)
        );

        let mut rows: Vec<_> = items
            .iter()
            .flat_map(|i| i.rows.iter().map(|&r| (i.kind, r)))
            .collect();
        let before = rows.len();
        rows.sort_unstable();
        rows.dedup();
        assert_eq!(rows.len(), before, "a Chronicle row maps to two items");
    }

    #[test]
    fn sorted_by_level_within_each_kind() {
        let items = catalog();
        for pair in items.windows(2).filter(|w| w[0].kind == w[1].kind) {
            assert!(
                pair[0].level() <= pair[1].level(),
                "{} before {}",
                pair[0].name,
                pair[1].name
            );
        }
        let first_rw = items.iter().find(|i| i.kind == Kind::Runeword).unwrap();
        assert!(first_rw.top_rune <= Some(9), "{} first", first_rw.name);
    }

    #[test]
    fn items_have_tiers_types_and_recipes() {
        let items = catalog();
        let get = |name| items.iter().find(|i| i.name == name).unwrap();
        let shako = get("Harlequin Crest");
        assert_eq!((shako.tier, shako.item_type), (Some(Tier::Elite), "Helm"));
        assert_eq!(get("Windforce").item_type, "Bow");
        assert_eq!(get("Annihilus").tier, None);
        assert_eq!(get("Spirit").base_or_runes(), "Tal Thul Ort Amn");
        assert!(items.iter().all(|i| !i.item_type.is_empty()));
        assert!(
            items
                .iter()
                .filter(|i| i.kind == Kind::Runeword)
                .all(|i| !i.runes.is_empty())
        );
        assert!(items.iter().all(|i| !i.rows.is_empty()));
        assert!(
            items
                .iter()
                .filter(|i| i.kind != Kind::Runeword)
                .all(|i| i.qlvl.parse::<u16>().is_ok())
        );
        assert_eq!(get("Azurewrath").qlvl, "87");
        assert_eq!(get("Horazon's Dominion").rows, [136]);
        assert_eq!(get("Enigma").top_rune, Some(31));
        assert_eq!(get("Enigma").top_rune_label, "31 Jah");
        assert_eq!(get("Steel").top_rune_label, "3 Tir");
    }

    #[test]
    fn every_item_has_details() {
        let items = catalog();
        let get = |name| &items.iter().find(|i| i.name == name).unwrap().details;
        assert!(
            items
                .iter()
                .all(|i| i.details.req_level.parse::<u8>().is_ok()),
            "missing level"
        );
        assert!(
            items.iter().all(|i| !i.details.stats.is_empty()),
            "missing stats"
        );

        let shako = get("Harlequin Crest");
        assert_eq!(shako.req_level, "62");
        assert_eq!(
            shako.stats[..2],
            ["+2 to All Skills", "+2 to all Attributes"]
        );

        let enigma = get("Enigma");
        assert_eq!(
            (enigma.req_level, enigma.sockets, enigma.bases),
            ("65", "3", "Body Armor")
        );
        // Includes the runes' own bonuses (Ber).
        assert!(
            enigma
                .stats
                .contains(&"Physical Damage Received Reduced by 8%")
        );

        assert_eq!(get("Azurewrath").req_level, "85");
        assert!(
            get("Tal Rasha's Guardianship")
                .set_bonuses
                .contains(&"(2 items) +10% Faster Cast Rate")
        );
    }

    #[test]
    fn every_set_has_bonuses() {
        let bonuses = set_bonuses();
        for item in catalog().iter().filter(|i| i.kind == Kind::Set) {
            let b = &bonuses
                .get(item.set)
                .unwrap_or_else(|| panic!("no bonuses for {}", item.set));
            assert!(!b.full.is_empty(), "no full set bonus for {}", item.set);
        }
        assert!(
            bonuses["Tal Rasha's Wrappings"]
                .full
                .contains(&"All Resistances +50")
        );
    }
}
