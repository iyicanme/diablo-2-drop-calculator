use super::raw::{get, get_i32, parse_tsv};
use crate::model::{Difficulty, MonsterType};
use std::collections::{HashMap, HashSet};

pub struct Area {
    /// Internal id (the `Name` column) — used to join against the hardcoded boss/superunique tables.
    pub id: String,
    /// Human-readable name (the `LevelName` column) — what the user wants in the CSV.
    pub display_name: String,
    pub act: i32,
    pub monster_levels: HashMap<Difficulty, i32>,
    pub monster_class_ids: HashMap<(Difficulty, MonsterType), HashSet<String>>,
}

fn read_mons(row: &super::raw::Row, prefix: &str) -> HashSet<String> {
    (1..=25).filter_map(|i| {
        let v = get(row, &format!("{prefix}{i}")).trim();
        if v.is_empty() { None } else { Some(v.to_string()) }
    }).collect()
}

const PANDEMONIUM_AREAS: [&str; 3] =
    ["Act 5 - Pandemonium 1", "Act 5 - Pandemonium 2", "Act 5 - Pandemonium 3"];

pub fn load_areas(
    data: &str,
    hardcoded_boss_areas: &HashMap<(&str, MonsterType), Vec<&str>>,
) -> HashMap<String, Area> {
    let rows = parse_tsv(data);
    let mut result = HashMap::new();
    for row in &rows {
        let id = get(row, "Name").to_string();
        let display_name = get(row, "LevelName").trim().to_string();
        if display_name.is_empty() {
            continue;
        }
        let act = get_i32(row, "Act").unwrap_or(0) + 1;

        let mut monster_levels = HashMap::new();
        if let Some(l) = get_i32(row, "MonLvlEx") {
            monster_levels.insert(Difficulty::Normal, l);
        }
        if let Some(l) = get_i32(row, "MonLvlEx(N)") {
            monster_levels.insert(Difficulty::Nightmare, l);
        }
        if let Some(l) = get_i32(row, "MonLvlEx(H)") {
            monster_levels.insert(Difficulty::Hell, l);
        }

        let mons = read_mons(row, "mon");
        let nmons = read_mons(row, "nmon");
        let umons = read_mons(row, "umon");

        let hardcoded_for = |mt: MonsterType| -> HashSet<String> {
            hardcoded_boss_areas
                .get(&(id.as_str(), mt))
                .map(|v| v.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default()
        };

        let mut monster_class_ids: HashMap<(Difficulty, MonsterType), HashSet<String>> = HashMap::new();
        let mut put = |difficulty: Difficulty, mt: MonsterType, ids: HashSet<String>| {
            if !ids.is_empty() {
                monster_class_ids.insert((difficulty, mt), ids);
            }
        };

        if !PANDEMONIUM_AREAS.contains(&id.as_str()) {
            put(Difficulty::Normal, MonsterType::Regular, &mons | &hardcoded_for(MonsterType::Regular));
            put(Difficulty::Normal, MonsterType::Champion, &umons | &hardcoded_for(MonsterType::Champion));
            put(Difficulty::Normal, MonsterType::Unique, &umons | &hardcoded_for(MonsterType::Unique));
            put(Difficulty::Normal, MonsterType::Boss, hardcoded_for(MonsterType::Boss));
            put(Difficulty::Nightmare, MonsterType::Regular, &nmons | &hardcoded_for(MonsterType::Regular));
            put(Difficulty::Nightmare, MonsterType::Champion, &nmons | &hardcoded_for(MonsterType::Champion));
            put(Difficulty::Nightmare, MonsterType::Unique, &nmons | &hardcoded_for(MonsterType::Unique));
            put(Difficulty::Nightmare, MonsterType::Boss, hardcoded_for(MonsterType::Boss));
        }
        put(Difficulty::Hell, MonsterType::Regular, &nmons | &hardcoded_for(MonsterType::Regular));
        put(Difficulty::Hell, MonsterType::Champion, &nmons | &hardcoded_for(MonsterType::Champion));
        put(Difficulty::Hell, MonsterType::Unique, &nmons | &hardcoded_for(MonsterType::Unique));
        put(Difficulty::Hell, MonsterType::Boss, hardcoded_for(MonsterType::Boss));

        result.insert(id.clone(), Area { id, display_name, act, monster_levels, monster_class_ids });
    }
    result
}
