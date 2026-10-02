use super::raw::{get, get_i32, is_one, parse_tsv};
use crate::model::{Difficulty, TreasureClassType};
use std::collections::HashMap;

pub struct MonsterClass {
    pub id: String,
    pub name_str: String,
    pub is_boss: bool,
    pub levels: HashMap<Difficulty, i32>,
    pub minion_ids: Vec<String>,
    pub treasure_classes: HashMap<(Difficulty, TreasureClassType), String>,
    pub resistances: HashMap<Difficulty, Resistances>,
    /// Display-only, for the web app's hover card.
    pub stats: HashMap<Difficulty, MonsterStats>,
    pub is_undead: bool,
    pub is_demon: bool,
}

/// Life and experience as monstats.txt stores them: percentages of monlvl's per-level base values.
#[derive(Clone, Copy, Default)]
pub struct MonsterStats {
    pub min_hp_pct: i32,
    pub max_hp_pct: i32,
    pub exp_pct: i32,
}

#[derive(Clone, Copy, Default)]
pub struct Resistances {
    pub physical: i32,
    pub magic: i32,
    pub fire: i32,
    pub lightning: i32,
    pub cold: i32,
    pub poison: i32,
}

fn parse_resistances(row: &super::raw::Row, suffix: &str) -> Resistances {
    Resistances {
        physical: get_i32(row, &format!("ResDm{suffix}")).unwrap_or(0),
        magic: get_i32(row, &format!("ResMa{suffix}")).unwrap_or(0),
        fire: get_i32(row, &format!("ResFi{suffix}")).unwrap_or(0),
        lightning: get_i32(row, &format!("ResLi{suffix}")).unwrap_or(0),
        cold: get_i32(row, &format!("ResCo{suffix}")).unwrap_or(0),
        poison: get_i32(row, &format!("ResPo{suffix}")).unwrap_or(0),
    }
}

pub fn load_monster_classes(data: &str) -> HashMap<String, MonsterClass> {
    let rows = parse_tsv(data);
    let mut result = HashMap::new();
    for row in &rows {
        let enabled = is_one(row, "enabled");
        let killable = is_one(row, "killable");
        let tc = get(row, "TreasureClass");
        let tc_n = get(row, "TreasureClass(N)");
        if !enabled || !killable || tc.trim().is_empty() || tc_n.trim().is_empty() {
            continue;
        }
        let id = get(row, "Id").to_string();
        let is_boss = is_one(row, "boss");
        let mut levels = HashMap::new();
        levels.insert(Difficulty::Normal, get_i32(row, "Level").unwrap_or(1));
        levels.insert(Difficulty::Nightmare, get_i32(row, "Level(N)").unwrap_or(1));
        levels.insert(Difficulty::Hell, get_i32(row, "Level(H)").unwrap_or(1));

        let minion1 = get(row, "minion1").to_string();
        let minion2 = get(row, "minion2").to_string();
        let minion_ids: Vec<String> = if minion1.is_empty() && minion2.is_empty() {
            vec![id.clone()]
        } else {
            [minion1, minion2].into_iter().filter(|s| !s.is_empty()).collect()
        };

        let mut treasure_classes = HashMap::new();
        let cannot_herald = get(row, "CannotHerald").trim() == "1";
        let mut add = |difficulty: Difficulty, tc_type: TreasureClassType, value: &str| {
            if !value.trim().is_empty() {
                treasure_classes.insert((difficulty, tc_type), value.trim().to_string());
            }
        };
        for (difficulty, suffix) in
            [(Difficulty::Normal, ""), (Difficulty::Nightmare, "(N)"), (Difficulty::Hell, "(H)")]
        {
            add(difficulty, TreasureClassType::Regular, get(row, &format!("TreasureClass{suffix}")));
            add(difficulty, TreasureClassType::Champion, get(row, &format!("TreasureClassChamp{suffix}")));
            add(difficulty, TreasureClassType::Unique, get(row, &format!("TreasureClassUnique{suffix}")));
            add(difficulty, TreasureClassType::Quest, get(row, &format!("TreasureClassQuest{suffix}")));
            add(
                difficulty,
                TreasureClassType::DesecratedRegular,
                get(row, &format!("TreasureClassDesecrated{suffix}")),
            );
            add(
                difficulty,
                TreasureClassType::DesecratedChampion,
                get(row, &format!("TreasureClassDesecratedChamp{suffix}")),
            );
            add(
                difficulty,
                TreasureClassType::DesecratedUnique,
                get(row, &format!("TreasureClassDesecratedUnique{suffix}")),
            );
            if !cannot_herald {
                add(difficulty, TreasureClassType::HeraldRegular, get(row, &format!("TreasureClassHerald{suffix}")));
            }
        }

        let mut stats = HashMap::new();
        for (difficulty, min_hp, max_hp, exp) in [
            (Difficulty::Normal, "minHP", "maxHP", "Exp"),
            (Difficulty::Nightmare, "MinHP(N)", "MaxHP(N)", "Exp(N)"),
            (Difficulty::Hell, "MinHP(H)", "MaxHP(H)", "Exp(H)"),
        ] {
            stats.insert(
                difficulty,
                MonsterStats {
                    min_hp_pct: get_i32(row, min_hp).unwrap_or(0),
                    max_hp_pct: get_i32(row, max_hp).unwrap_or(0),
                    exp_pct: get_i32(row, exp).unwrap_or(0),
                },
            );
        }

        let mut resistances = HashMap::new();
        resistances.insert(Difficulty::Normal, parse_resistances(row, ""));
        resistances.insert(Difficulty::Nightmare, parse_resistances(row, "(N)"));
        resistances.insert(Difficulty::Hell, parse_resistances(row, "(H)"));

        result.insert(
            id.clone(),
            MonsterClass {
                id,
                name_str: get(row, "NameStr").trim().to_string(),
                is_boss,
                levels,
                minion_ids,
                treasure_classes,
                resistances,
                stats,
                is_undead: is_one(row, "lUndead") || is_one(row, "hUndead"),
                is_demon: is_one(row, "demon"),
            },
        );
    }
    result
}
