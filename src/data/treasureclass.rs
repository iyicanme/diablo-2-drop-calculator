use super::items::ItemDatabase;
use super::raw::{get, get_i32, get_i64, parse_tsv};
use crate::model::{Difficulty, QualityRatios};
use std::collections::HashMap;

pub struct TreasureClassRow {
    pub name: String,
    pub group: Option<i32>,
    pub level: Option<i32>,
    pub picks: i32,
    pub ratios: QualityRatios,
    pub no_drop: Option<i64>,
    /// (leaf name — another TC's name, a virtual-TC bucket name, or a concrete base item code, weight)
    pub items: Vec<(String, i64)>,
}

impl TreasureClassRow {
    /// Sum of item weights (NOT including NoDrop) — the "probabilityDenominator".
    pub fn items_denominator(&self) -> i64 {
        self.items.iter().map(|(_, w)| w).sum()
    }
}

fn parse_treasure_classes(data: &str) -> Vec<TreasureClassRow> {
    let rows = parse_tsv(data);
    let mut result = Vec::new();
    for row in &rows {
        let name = get(row, "Treasure Class").to_string();
        if name.is_empty() {
            continue;
        }
        let mut items = Vec::new();
        for i in 1..=10 {
            let item_raw = get(row, &format!("Item{i}"));
            if item_raw.trim().is_empty() {
                continue;
            }
            let item = ItemDatabase::normalize_leaf_code(item_raw);
            if item.is_empty() {
                continue;
            }
            let prob = get_i64(row, &format!("Prob{i}")).unwrap_or(0);
            items.push((item, prob));
        }
        result.push(TreasureClassRow {
            name,
            group: get_i32(row, "group"),
            level: get_i32(row, "level"),
            picks: get_i32(row, "Picks").unwrap_or(1),
            ratios: QualityRatios {
                unique: get_i64(row, "Unique").unwrap_or(0),
                set: get_i64(row, "Set").unwrap_or(0),
                rare: get_i64(row, "Rare").unwrap_or(0),
                magic: get_i64(row, "Magic").unwrap_or(0),
            },
            no_drop: get_i64(row, "NoDrop"),
            items,
        });
    }
    result
}

pub struct TreasureClassLibrary {
    pub by_name: HashMap<String, TreasureClassRow>,
    /// Group id -> TC names sorted by ascending `level`, for the same-group upgrade ladder walk.
    by_group: HashMap<i32, Vec<String>>,
}

impl TreasureClassLibrary {
    pub fn load(data: &str) -> Self {
        let rows = parse_treasure_classes(data);
        let mut by_group: HashMap<i32, Vec<(i32, String)>> = HashMap::new();
        for row in &rows {
            if let (Some(group), Some(level)) = (row.group, row.level) {
                by_group.entry(group).or_default().push((level, row.name.clone()));
            }
        }
        for names in by_group.values_mut() {
            names.sort_by_key(|(level, _)| *level);
        }
        let by_group = by_group.into_iter().map(|(g, v)| (g, v.into_iter().map(|(_, n)| n).collect())).collect();
        let by_name = rows.into_iter().map(|r| (r.name.clone(), r)).collect();
        TreasureClassLibrary { by_name, by_group }
    }

    /// Walks a same-`group` ladder of treasure classes forward while the next tier's `level` is <= `target_level`,
    /// mirroring how a monster's assigned TC gets upgraded for Nightmare/Hell (and, forced, for Terror Zones).
    pub fn upgrade_for_level(&self, tc_name: &str, target_level: i32) -> String {
        let Some(base) = self.by_name.get(tc_name) else {
            return tc_name.to_string();
        };
        let Some(group) = base.group else {
            return tc_name.to_string();
        };
        let Some(ladder) = self.by_group.get(&group) else {
            return tc_name.to_string();
        };
        let Some(base_index) = ladder.iter().position(|n| n == tc_name) else {
            return tc_name.to_string();
        };
        let mut result = tc_name.to_string();
        let mut next_index = base_index + 1;
        while next_index < ladder.len() {
            let next_name = &ladder[next_index];
            let next_level = self.by_name.get(next_name).and_then(|r| r.level).unwrap_or(i32::MAX);
            if next_level > target_level {
                break;
            }
            result = next_name.clone();
            next_index += 1;
        }
        result
    }

    pub fn resolved_tc_for_monster(
        &self,
        base_tc_name: &str,
        monster_level: i32,
        difficulty: Difficulty,
        always_upgrade: bool,
    ) -> String {
        if !always_upgrade && difficulty == Difficulty::Normal {
            return base_tc_name.to_string();
        }
        self.upgrade_for_level(base_tc_name, monster_level)
    }
}
