use super::raw::{get, get_i32, parse_tsv};
use crate::model::{Difficulty, MonsterType, TreasureClassType};
use std::collections::HashMap;

pub struct SuperUnique {
    pub id: String,
    pub name: String,
    pub monster_class_id: String,
    pub area_id: String,
    pub has_minions: bool,
    pub treasure_classes: HashMap<(Difficulty, TreasureClassType), String>,
}

/// Act bosses aren't listed in levels.txt's generic mon/nmon/umon population columns — these are
/// game facts (which area each act boss occupies) transcribed independently for this project.
pub fn hardcoded_boss_areas() -> HashMap<(&'static str, MonsterType), Vec<&'static str>> {
    let regular: &[(&str, &[&str])] = &[("Act 5 - Temple Boss", &["minion6"])];
    let boss: &[(&str, &[&str])] = &[
        ("Act 1 - Catacombs 4", &["andariel"]),
        ("Act 2 - Duriel's Lair", &["duriel"]),
        ("Act 2 - Sewer 1 C", &["radament"]),
        ("Act 3 - Mephisto 3", &["mephisto"]),
        ("Act 4 - Diablo 1", &["diablo"]),
        ("Act 2 - Arcane", &["summoner"]),
        ("Act 4 - Mesa 2", &["izual"]),
        ("Act 1 - Graveyard", &["bloodraven"]),
        ("Act 1 - Tristram", &["griswold"]),
        ("Act 5 - World Stone", &["baalcrab"]),
        ("Act 5 - Temple 2", &["putriddefiler1"]),
        ("Act 5 - Temple Boss", &["putriddefiler1", "putriddefiler2", "nihlathakboss"]),
        ("Act 5 - Baal Temple 1", &["putriddefiler2", "putriddefiler3"]),
        ("Act 5 - Baal Temple 3", &["putriddefiler3", "putriddefiler4", "putriddefiler5"]),
    ];
    let mut result: HashMap<(&str, MonsterType), Vec<&str>> = HashMap::new();
    for (area, ids) in regular {
        result.entry((area, MonsterType::Regular)).or_default().extend(ids.iter());
    }
    for (area, ids) in boss {
        result.entry((area, MonsterType::Boss)).or_default().extend(ids.iter());
    }
    result
}

/// Named superuniques' spawn area isn't in superuniques.txt itself — transcribed independently.
pub fn hardcoded_superunique_areas() -> HashMap<&'static str, &'static str> {
    [
        ("Bishibosh", "Act 1 - Wilderness 2"),
        ("Bonebreak", "Act 1 - Crypt 1 A"),
        ("Coldcrow", "Act 1 - Cave 2"),
        ("Rakanishu", "Act 1 - Wilderness 3"),
        ("Treehead WoodFist", "Act 1 - Wilderness 4"),
        ("Griswold", "Act 1 - Tristram"),
        ("The Countess", "Act 1 - Crypt 3 E"),
        ("Pitspawn Fouldog", "Act 1 - Jail 2"),
        ("Boneash", "Act 1 - Cathedral"),
        ("Radament", "Act 2 - Sewer 1 C"),
        ("Bloodwitch the Wild", "Act 2 - Tomb 2 Treasure"),
        ("Fangskin", "Act 2 - Tomb 3 Treasure"),
        ("Beetleburst", "Act 2 - Desert 3"),
        ("Leatherarm", "Act 2 - Tomb 1 Treasure"),
        ("Coldworm the Burrower", "Act 2 - Lair 1 Treasure"),
        ("Fire Eye", "Act 2 - Basement 3"),
        ("Dark Elder", "Act 2 - Desert 4"),
        ("The Summoner", "Act 2 - Arcane"),
        ("Ancient Kaa the Soulless", "Act 2 - Tomb Tal 1"),
        ("The Smith", "Act 1 - Barracks"),
        ("Web Mage the Burning", "Act 3 - Spider 2"),
        ("Witch Doctor Endugu", "Act 3 - Dungeon 2 Treasure"),
        ("Stormtree", "Act 3 - Kurast 1"),
        ("Sarina the Battlemaid", "Act 3 - Temple 1"),
        ("Icehawk Riftwing", "Act 3 - Sewer 1"),
        ("Ismail Vilehand", "Act 3 - Travincal"),
        ("Geleb Flamefinger", "Act 3 - Travincal"),
        ("Bremm Sparkfist", "Act 3 - Mephisto 3"),
        ("Toorc Icefist", "Act 3 - Travincal"),
        ("Wyand Voidfinger", "Act 3 - Mephisto 3"),
        ("Maffer Dragonhand", "Act 3 - Mephisto 3"),
        ("Infector of Souls", "Act 4 - Diablo 1"),
        ("Lord De Seis", "Act 4 - Diablo 1"),
        ("Grand Vizier of Chaos", "Act 4 - Diablo 1"),
        ("The Cow King", "Act 1 - Moo Moo Farm"),
        ("Corpsefire", "Act 1 - Cave 1"),
        ("The Feature Creep", "Act 4 - Lava 1"),
        ("Siege Boss", "Act 5 - Siege 1"),
        ("Ancient Barbarian 1", "Act 5 - Mountain Top"),
        ("Ancient Barbarian 2", "Act 5 - Mountain Top"),
        ("Ancient Barbarian 3", "Act 5 - Mountain Top"),
        ("Bonesaw Breaker", "Act 5 - Ice Cave 2"),
        ("Dac Farren", "Act 5 - Siege 1"),
        ("Megaflow Rectifier", "Act 5 - Barricade 1"),
        ("Eyeback Unleashed", "Act 5 - Barricade 1"),
        ("Threash Socket", "Act 5 - Barricade 2"),
        ("Pindleskin", "Act 5 - Temple Entrance"),
        ("Snapchip Shatter", "Act 5 - Ice Cave 3A"),
        ("Sharp Tooth Sayer", "Act 5 - Barricade 1"),
        ("Frozenstein", "Act 5 - Ice Cave 1A"),
        ("Nihlathak Boss", "Act 5 - Temple Boss"),
        ("Baal Subject 1", "Act 5 - Throne Room"),
        ("Baal Subject 2", "Act 5 - Throne Room"),
        ("Baal Subject 3", "Act 5 - Throne Room"),
        ("Baal Subject 4", "Act 5 - Throne Room"),
        ("Baal Subject 5", "Act 5 - Throne Room"),
    ]
    .into_iter()
    .collect()
}

pub fn load_superuniques(data: &str, areas_by_superunique_id: &HashMap<&str, &str>) -> Vec<SuperUnique> {
    let rows = parse_tsv(data);
    let mut result = Vec::new();
    for row in &rows {
        let id = get(row, "Superunique").to_string();
        let name = get(row, "Name").trim().to_string();
        let monster_class_id = get(row, "Class").to_string();
        let has_minions = get_i32(row, "MaxGrp").unwrap_or(0) > 0;
        let normal_tc = get(row, "TC");
        let Some(area_id) = areas_by_superunique_id.get(id.as_str()) else {
            continue;
        };
        if name.is_empty() || monster_class_id.is_empty() || normal_tc.trim().is_empty() {
            continue;
        }
        let mut treasure_classes = HashMap::new();
        let mut add = |difficulty: Difficulty, tc_type: TreasureClassType, value: &str| {
            if !value.trim().is_empty() {
                treasure_classes.insert((difficulty, tc_type), value.trim().to_string());
            }
        };
        add(Difficulty::Normal, TreasureClassType::Regular, get(row, "TC"));
        add(Difficulty::Normal, TreasureClassType::DesecratedRegular, get(row, "TC Desecrated"));
        add(Difficulty::Nightmare, TreasureClassType::Regular, get(row, "TC(N)"));
        add(Difficulty::Nightmare, TreasureClassType::DesecratedRegular, get(row, "TC(N) Desecrated"));
        add(Difficulty::Hell, TreasureClassType::Regular, get(row, "TC(H)"));
        add(Difficulty::Hell, TreasureClassType::DesecratedRegular, get(row, "TC(H) Desecrated"));

        result.push(SuperUnique {
            id,
            name,
            monster_class_id,
            area_id: area_id.to_string(),
            has_minions,
            treasure_classes,
        });
    }
    result
}
