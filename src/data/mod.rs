pub mod items;
pub mod levels;
pub mod monsters;
pub mod props;
pub mod raw;
pub mod strings;
pub mod superuniques;
pub mod treasureclass;

use items::ItemDatabase;
use levels::Area;
use monsters::MonsterClass;
use std::collections::HashMap;
use crate::model::Difficulty;
use superuniques::SuperUnique;
use treasureclass::TreasureClassLibrary;

const MONSTATS_TXT: &str = include_str!("../../data/monstats.txt");
const LEVELS_TXT: &str = include_str!("../../data/levels.txt");
const TREASURECLASSEX_TXT: &str = include_str!("../../data/treasureclassex.txt");
const SUPERUNIQUES_TXT: &str = include_str!("../../data/superuniques.txt");
const WEAPONS_TXT: &str = include_str!("../../data/weapons.txt");
const ARMOR_TXT: &str = include_str!("../../data/armor.txt");
const MISC_TXT: &str = include_str!("../../data/misc.txt");
const ITEMTYPES_TXT: &str = include_str!("../../data/itemtypes.txt");
const ITEMRATIO_TXT: &str = include_str!("../../data/itemratio.txt");
const UNIQUEITEMS_TXT: &str = include_str!("../../data/uniqueitems.txt");
const SETITEMS_TXT: &str = include_str!("../../data/setitems.txt");

const MONSTERS_JSON: &str = include_str!("../../data/monsters.json");
const ITEM_NAMES_JSON: &str = include_str!("../../data/item-names.json");
const ITEM_RUNES_JSON: &str = include_str!("../../data/item-runes.json");
const LEVELS_JSON: &str = include_str!("../../data/levels.json");

// Display-only data for the web app's hover cards (blizzhackers/d2data, see data/SOURCE.md).
const MONLVL_JSON: &str = include_str!("../../data/monlvl.json");
const PROPERTIES_JSON: &str = include_str!("../../data/properties.json");
const SKILLS_JSON: &str = include_str!("../../data/skills.json");
const SKILLDESC_JSON: &str = include_str!("../../data/skilldesc.json");
const LOCALESTRINGS_ENG_JSON: &str = include_str!("../../data/localestrings-eng.json");
const GEMS_JSON: &str = include_str!("../../data/gems.json");

pub struct GameData {
    pub monster_classes: HashMap<String, MonsterClass>,
    pub areas: HashMap<String, Area>,
    pub superuniques: Vec<SuperUnique>,
    pub treasure_classes: TreasureClassLibrary,
    pub items: ItemDatabase,
    pub monster_names: HashMap<String, String>,
}

/// Per-level base life and experience (monlvl.json's expansion `L-` columns), which monstats.txt's
/// HP/Exp percentages apply to. Indexed by monster level.
pub struct MonsterLevelTable {
    pub hp: HashMap<Difficulty, Vec<i64>>,
    pub xp: HashMap<Difficulty, Vec<i64>>,
}

impl MonsterLevelTable {
    pub fn load(data: &str) -> Self {
        let rows: HashMap<String, serde_json::Value> =
            serde_json::from_str(data.strip_prefix('\u{FEFF}').unwrap_or(data)).expect("monlvl.json");
        let mut by_level: Vec<(i64, &serde_json::Value)> =
            rows.values().filter_map(|r| Some((r.get("Level")?.as_i64()?, r))).collect();
        by_level.sort_by_key(|(level, _)| *level);
        let column = |name: &str| -> Vec<i64> {
            by_level.iter().map(|(_, r)| r.get(name).and_then(|v| v.as_i64()).unwrap_or(0)).collect()
        };
        let mut hp = HashMap::new();
        let mut xp = HashMap::new();
        for (difficulty, suffix) in [(Difficulty::Normal, ""), (Difficulty::Nightmare, "(N)"), (Difficulty::Hell, "(H)")] {
            hp.insert(difficulty, column(&format!("L-HP{suffix}")));
            xp.insert(difficulty, column(&format!("L-XP{suffix}")));
        }
        MonsterLevelTable { hp, xp }
    }
}

/// Everything the hover cards need beyond the drop tables. Loaded separately from `GameData` so
/// the CLI never parses it.
pub struct DisplayData {
    pub monster_levels: MonsterLevelTable,
    pub props: props::PropRenderer,
    /// Rune/gem code -> (socket slot label, properties granted when socketed there).
    pub socket_bonuses: HashMap<String, Vec<(&'static str, Vec<items::RawProp>)>>,
}

/// gems.json: up to three `{slot}Mod{n}Code/Param/Min/Max` properties per socket slot.
fn load_socket_bonuses(data: &str) -> HashMap<String, Vec<(&'static str, Vec<items::RawProp>)>> {
    let gems: HashMap<String, serde_json::Value> =
        serde_json::from_str(data.strip_prefix('\u{FEFF}').unwrap_or(data)).expect("gems.json");
    let text = |v: Option<&serde_json::Value>| match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    gems.values()
        .filter_map(|g| {
            let code = g.get("code")?.as_str()?.to_string();
            let slots = [("weapon", "Weapons"), ("helm", "Armor and helms"), ("shield", "Shields")]
                .into_iter()
                .map(|(key, label)| {
                    let props = (1..=3)
                        .filter_map(|n| {
                            let prop = text(g.get(&format!("{key}Mod{n}Code")));
                            (!prop.is_empty()).then(|| items::RawProp {
                                code: prop,
                                par: text(g.get(&format!("{key}Mod{n}Param"))),
                                min: text(g.get(&format!("{key}Mod{n}Min"))),
                                max: text(g.get(&format!("{key}Mod{n}Max"))),
                            })
                        })
                        .collect();
                    (label, props)
                })
                .collect();
            Some((code, slots))
        })
        .collect()
}

impl DisplayData {
    pub fn load() -> Self {
        DisplayData {
            monster_levels: MonsterLevelTable::load(MONLVL_JSON),
            props: props::PropRenderer::load(PROPERTIES_JSON, SKILLS_JSON, SKILLDESC_JSON, LOCALESTRINGS_ENG_JSON),
            socket_bonuses: load_socket_bonuses(GEMS_JSON),
        }
    }
}

impl GameData {
    pub fn load() -> Self {
        let monster_classes = monsters::load_monster_classes(MONSTATS_TXT);
        let hardcoded_boss_areas = superuniques::hardcoded_boss_areas();
        let mut areas = levels::load_areas(LEVELS_TXT, &hardcoded_boss_areas);
        let hardcoded_superunique_areas = superuniques::hardcoded_superunique_areas();
        let mut superuniques = superuniques::load_superuniques(SUPERUNIQUES_TXT, &hardcoded_superunique_areas);
        let treasure_classes = TreasureClassLibrary::load(TREASURECLASSEX_TXT);

        let mut item_names = strings::load_string_table(ITEM_NAMES_JSON);
        item_names.extend(strings::load_string_table(ITEM_RUNES_JSON));
        let items = ItemDatabase::load(
            WEAPONS_TXT,
            ARMOR_TXT,
            MISC_TXT,
            ITEMTYPES_TXT,
            ITEMRATIO_TXT,
            UNIQUEITEMS_TXT,
            SETITEMS_TXT,
            item_names,
        );

        let monster_names = strings::load_string_table(MONSTERS_JSON);

        // levels.txt / superuniques.txt hold string keys, not the names the game shows (e.g. the
        // "Chaos Sanctum" key is displayed as "The Chaos Sanctuary"). Keys without an entry are kept.
        let level_names = strings::load_string_table(LEVELS_JSON);
        for area in areas.values_mut() {
            if let Some(name) = level_names.get(&area.display_name) {
                area.display_name = name.clone();
            }
        }
        for superunique in &mut superuniques {
            if let Some(name) = monster_names.get(&superunique.name) {
                superunique.name = name.clone();
            }
        }

        GameData { monster_classes, areas, superuniques, treasure_classes, items, monster_names }
    }

    pub fn monster_display_name(&self, name_str: &str) -> String {
        self.monster_names.get(name_str).cloned().unwrap_or_else(|| name_str.to_string())
    }
}
