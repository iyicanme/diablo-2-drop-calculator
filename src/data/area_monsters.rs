use super::levels::Area;
use super::monsters::MonsterClass;
use super::raw::{get, get_i32, parse_tsv};
use crate::model::{Difficulty, MonsterType};
use std::collections::HashMap;

/// How many monsters of each kind clearing an area spawns on average (`area_monsters.tsv`).
/// `monster` is a monstats id, or a superuniques.txt key for the `superunique` kind; the kinds are
/// `normal`, `champion` (pack leaders and their champion minions), `unique`, `minion`,
/// `superunique` and `boss`.
#[derive(Default)]
pub struct AreaSpawns {
    counts: HashMap<(i32, Difficulty, String, String), f64>,
}

impl AreaSpawns {
    pub fn load(data: &str) -> Self {
        let mut counts = HashMap::new();
        for row in parse_tsv(data) {
            let difficulty = match get(&row, "difficulty") {
                "normal" => Difficulty::Normal,
                "nightmare" => Difficulty::Nightmare,
                "hell" => Difficulty::Hell,
                _ => continue,
            };
            let (Some(level_id), Ok(count)) = (
                get_i32(&row, "level_id"),
                get(&row, "count").trim().parse::<f64>(),
            ) else {
                continue;
            };
            counts.insert(
                (
                    level_id,
                    difficulty,
                    get(&row, "monster").to_string(),
                    get(&row, "kind").to_string(),
                ),
                count,
            );
        }
        AreaSpawns { counts }
    }

    pub fn get(&self, level_id: i32, difficulty: Difficulty, monster: &str, kind: &str) -> f64 {
        self.counts
            .get(&(level_id, difficulty, monster.to_string(), kind.to_string()))
            .copied()
            .unwrap_or(0.0)
    }

    /// levels.txt lists the monsters an area picks from, but some only arrive through another's
    /// spawn (e.g. Foul Crows from crow nests). Adds those to the areas they spawn in, in the
    /// difficulties where the area has monsters at all (so Pandemonium stays Hell-only).
    pub fn add_unlisted_monsters(
        &self,
        areas: &mut HashMap<String, Area>,
        monster_classes: &HashMap<String, MonsterClass>,
    ) {
        let by_level: HashMap<i32, String> =
            areas.values().map(|a| (a.level_id, a.id.clone())).collect();
        for ((level_id, difficulty, monster, kind), _) in &self.counts {
            let monster_type = match kind.as_str() {
                "normal" => MonsterType::Regular,
                "champion" => MonsterType::Champion,
                "unique" => MonsterType::Unique,
                _ => continue,
            };
            let Some(area) = by_level.get(level_id).and_then(|id| areas.get_mut(id)) else {
                continue;
            };
            if !monster_classes.contains_key(monster)
                || !area
                    .monster_class_ids
                    .contains_key(&(*difficulty, MonsterType::Regular))
            {
                continue;
            }
            area.monster_class_ids
                .entry((*difficulty, monster_type))
                .or_default()
                .insert(monster.clone());
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&(i32, Difficulty, String, String), &f64)> {
        self.counts.iter()
    }
}
