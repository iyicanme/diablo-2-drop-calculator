use crate::data::items::NamedItem;
use crate::data::levels::Area;
use crate::data::monsters::{MonsterClass, Resistances};
use crate::data::GameData;
use crate::engine::{quality, tc_resolver, terror};
use crate::export::DropRow;
use crate::model::{Difficulty, MonsterType, QualityRatios, TreasureClassType};
use std::collections::{HashMap, HashSet};

/// Resistance value at/above which a monster is treated as immune to that element in the CSV's
/// yes/no columns. This is a community convention, not something D2's drop math itself defines.
const IMMUNE_THRESHOLD: i32 = 100;

pub struct Config {
    pub players: i32,
    pub magic_find: i64,
    pub character_level: i32,
}

fn desecrated_type_for(tc_type: TreasureClassType) -> Option<TreasureClassType> {
    match tc_type {
        TreasureClassType::Regular => Some(TreasureClassType::DesecratedRegular),
        TreasureClassType::Champion => Some(TreasureClassType::DesecratedChampion),
        TreasureClassType::Unique => Some(TreasureClassType::DesecratedUnique),
        _ => None,
    }
}

fn compute_level(monster_class: &MonsterClass, difficulty: Difficulty, monster_type: MonsterType, area: &Area) -> i32 {
    let base = if difficulty == Difficulty::Normal || monster_type == MonsterType::Boss {
        *monster_class.levels.get(&difficulty).unwrap_or(&1)
    } else {
        *area.monster_levels.get(&difficulty).unwrap_or(&1)
    };
    base + monster_type.level_adjustment()
}

/// Minions escorting a unique pack or a superunique. They roll their own class's *regular* treasure
/// class, but at the leader's level +3 rather than the area's level: in Normal (and under a boss
/// leader) that's the leader's monstats level, otherwise the area level. Boss leaders give no +3.
/// The higher level doesn't change the NoDrop odds; it unlocks more unique/set items (qlvl <= mlvl),
/// raises the quality roll, and in Nightmare/Hell can upgrade the TC to a better tier.
fn minion_level(leader: &MonsterClass, difficulty: Difficulty, area: &Area) -> i32 {
    let base = if difficulty == Difficulty::Normal || leader.is_boss {
        *leader.levels.get(&difficulty).unwrap_or(&1)
    } else {
        *area.monster_levels.get(&difficulty).unwrap_or(&1)
    };
    base + if leader.is_boss { 0 } else { MonsterType::Minion.level_adjustment() }
}

/// (minion class, area, difficulty, terrorized, level): two leaders can field identical minions
/// (e.g. Fallen and Fallen Shaman packs both bring Fallen), which would just duplicate rows.
type MinionKey = (String, String, Difficulty, bool, i32);

#[allow(clippy::too_many_arguments)]
fn process_minions(
    game_data: &GameData,
    config: &Config,
    leader: &MonsterClass,
    area: &Area,
    difficulty: Difficulty,
    seen: &mut HashSet<MinionKey>,
    tc_cache: &mut HashMap<String, HashMap<String, (f64, QualityRatios)>>,
    emit: &mut dyn FnMut(DropRow),
) {
    let level = minion_level(leader, difficulty, area);
    for minion_id in &leader.minion_ids {
        let Some(minion) = game_data.monster_classes.get(minion_id) else { continue };
        let display_name = game_data.monster_display_name(&minion.name_str);
        let resistances = *minion.resistances.get(&difficulty).unwrap_or(&Resistances::default());

        if let Some(base_tc) = minion.treasure_classes.get(&(difficulty, TreasureClassType::Regular)) {
            if seen.insert((minion.id.clone(), area.id.clone(), difficulty, false, level)) {
                let instance = MonsterInstance {
                    display_name: display_name.clone(),
                    monster_class_id: minion.id.clone(),
                    monster_type: MonsterType::Minion,
                    area,
                    difficulty,
                    resistances,
                    terrorized: false,
                    level,
                    resolved_tc: game_data.treasure_classes.resolved_tc_for_monster(base_tc, level, difficulty, false),
                };
                process_instance(game_data, config, config.players, &instance, tc_cache, emit);
            }
        }

        // Terrorized minions follow the same rule as other monsters here: only classes with a
        // desecrated TC get a Terror Zone row.
        if let Some(desecrated_base) = minion.treasure_classes.get(&(difficulty, TreasureClassType::DesecratedRegular)) {
            let d_level = terror::desecrated_level(MonsterType::Minion, difficulty, level, config.character_level);
            if seen.insert((minion.id.clone(), area.id.clone(), difficulty, true, d_level)) {
                let instance = MonsterInstance {
                    display_name,
                    monster_class_id: minion.id.clone(),
                    monster_type: MonsterType::Minion,
                    area,
                    difficulty,
                    resistances,
                    terrorized: true,
                    level: d_level,
                    resolved_tc: game_data.treasure_classes.upgrade_for_level(desecrated_base, d_level),
                };
                process_instance(game_data, config, config.players, &instance, tc_cache, emit);
            }
        }
    }
}

fn resist_yes(value: i32) -> bool {
    value >= IMMUNE_THRESHOLD
}

struct MonsterInstance<'a> {
    display_name: String,
    monster_class_id: String,
    monster_type: MonsterType,
    area: &'a Area,
    difficulty: Difficulty,
    resistances: Resistances,
    terrorized: bool,
    level: i32,
    resolved_tc: String,
}

pub fn generate(game_data: &GameData, config: &Config) -> Vec<DropRow> {
    let mut rows = Vec::new();
    generate_with(game_data, config, |row| rows.push(row));
    rows
}

/// Like `generate`, but hands each row to `emit` as it's produced instead of collecting them, so
/// callers that repack rows (e.g. the wasm build) never hold the full `Vec<DropRow>` in memory.
pub fn generate_with(game_data: &GameData, config: &Config, mut emit: impl FnMut(DropRow)) {
    let emit: &mut dyn FnMut(DropRow) = &mut emit;
    let party_size = config.players;
    let mut tc_cache: HashMap<String, HashMap<String, (f64, QualityRatios)>> = HashMap::new();
    let mut seen_minions: HashSet<MinionKey> = HashSet::new();

    // Reverse index: (monster_class_id, difficulty, monster_type) -> areas it spawns in.
    let mut areas_by_monster: HashMap<(&str, Difficulty, MonsterType), Vec<&Area>> = HashMap::new();
    for area in game_data.areas.values() {
        for ((difficulty, monster_type), ids) in &area.monster_class_ids {
            for id in ids {
                areas_by_monster.entry((id.as_str(), *difficulty, *monster_type)).or_default().push(area);
            }
        }
    }

    for monster_class in game_data.monster_classes.values() {
        let display_name = game_data.monster_display_name(&monster_class.name_str);
        for (&(difficulty, tc_type), base_tc_name) in &monster_class.treasure_classes {
            if tc_type.is_desecrated() || tc_type == TreasureClassType::Quest || tc_type == TreasureClassType::HeraldRegular {
                continue;
            }
            for &monster_type in tc_type.valid_monster_types() {
                let Some(areas) = areas_by_monster.get(&(monster_class.id.as_str(), difficulty, monster_type)) else {
                    continue;
                };
                for area in areas {
                    let level = compute_level(monster_class, difficulty, monster_type, area);
                    let resolved_tc = game_data.treasure_classes.resolved_tc_for_monster(base_tc_name, level, difficulty, false);
                    let resistances = *monster_class.resistances.get(&difficulty).unwrap_or(&Resistances::default());

                    let instance = MonsterInstance {
                        display_name: display_name.clone(),
                        monster_class_id: monster_class.id.clone(),
                        monster_type,
                        area,
                        difficulty,
                        resistances,
                        terrorized: false,
                        level,
                        resolved_tc,
                    };
                    process_instance(game_data, config, party_size, &instance, &mut tc_cache, emit);

                    if let Some(desecrated_type) = desecrated_type_for(tc_type) {
                        if let Some(desecrated_base) = monster_class.treasure_classes.get(&(difficulty, desecrated_type)) {
                            let d_level = terror::desecrated_level(monster_type, difficulty, level, config.character_level);
                            let resolved = game_data.treasure_classes.upgrade_for_level(desecrated_base, d_level);
                            let t_instance = MonsterInstance {
                                display_name: display_name.clone(),
                                monster_class_id: monster_class.id.clone(),
                                monster_type,
                                area,
                                difficulty,
                                resistances,
                                terrorized: true,
                                level: d_level,
                                resolved_tc: resolved,
                            };
                            process_instance(game_data, config, party_size, &t_instance, &mut tc_cache, emit);
                        }
                    }

                    // Unique packs always spawn with minions.
                    if monster_type == MonsterType::Unique {
                        process_minions(game_data, config, monster_class, area, difficulty, &mut seen_minions, &mut tc_cache, emit);
                    }
                }
            }
        }
    }

    for superunique in &game_data.superuniques {
        let Some(monster_class) = game_data.monster_classes.get(&superunique.monster_class_id) else { continue };
        let Some(area) = game_data.areas.get(&superunique.area_id) else { continue };
        for difficulty in Difficulty::ALL {
            let Some(base_tc) = superunique.treasure_classes.get(&(difficulty, TreasureClassType::Regular)) else {
                continue;
            };
            let level = compute_level(monster_class, difficulty, MonsterType::SuperUnique, area);
            let resolved_tc = game_data.treasure_classes.resolved_tc_for_monster(base_tc, level, difficulty, false);
            let resistances = *monster_class.resistances.get(&difficulty).unwrap_or(&Resistances::default());

            let instance = MonsterInstance {
                display_name: superunique.name.clone(),
                monster_class_id: monster_class.id.clone(),
                monster_type: MonsterType::SuperUnique,
                area,
                difficulty,
                resistances,
                terrorized: false,
                level,
                resolved_tc,
            };
            process_instance(game_data, config, party_size, &instance, &mut tc_cache, emit);

            if let Some(desecrated_base) = superunique.treasure_classes.get(&(difficulty, TreasureClassType::DesecratedRegular)) {
                let d_level = terror::desecrated_level(MonsterType::SuperUnique, difficulty, level, config.character_level);
                let resolved = game_data.treasure_classes.upgrade_for_level(desecrated_base, d_level);
                let t_instance = MonsterInstance {
                    display_name: superunique.name.clone(),
                    monster_class_id: monster_class.id.clone(),
                    monster_type: MonsterType::SuperUnique,
                    area,
                    difficulty,
                    resistances,
                    terrorized: true,
                    level: d_level,
                    resolved_tc: resolved,
                };
                process_instance(game_data, config, party_size, &t_instance, &mut tc_cache, emit);
            }

            if superunique.has_minions {
                process_minions(game_data, config, monster_class, area, difficulty, &mut seen_minions, &mut tc_cache, emit);
            }
        }
    }
}

fn eligible_named<'a>(candidates: Option<&'a Vec<NamedItem>>, level: i32, monster_class_id: &str) -> Vec<&'a NamedItem> {
    candidates
        .map(|v| {
            v.iter()
                .filter(|c| c.level <= level)
                .filter(|c| c.only_drops_from_monster_class.as_deref().is_none_or(|m| m == monster_class_id))
                .collect()
        })
        .unwrap_or_default()
}

fn process_instance(
    game_data: &GameData,
    config: &Config,
    party_size: i32,
    instance: &MonsterInstance,
    tc_cache: &mut HashMap<String, HashMap<String, (f64, QualityRatios)>>,
    emit: &mut dyn FnMut(DropRow),
) {
    let items = &game_data.items;
    let resolved = tc_cache.entry(instance.resolved_tc.clone()).or_insert_with(|| {
        tc_resolver::resolve_to_base_items(&game_data.treasure_classes, items, &instance.resolved_tc, config.players, party_size)
    });

    let r = &instance.resistances;
    let base_row = DropRow {
        monster_name: instance.display_name.clone(),
        monster_type: instance.monster_type.label(),
        monster_location: instance.area.display_name.clone(),
        act: instance.area.act,
        difficulty: instance.difficulty.label(),
        terrorized: instance.terrorized,
        res_fire: resist_yes(r.fire),
        res_cold: resist_yes(r.cold),
        res_lightning: resist_yes(r.lightning),
        res_poison: resist_yes(r.poison),
        res_magic: resist_yes(r.magic),
        res_physical: resist_yes(r.physical),
        item_name: String::new(),
        item_tc: instance.resolved_tc.clone(),
        item_type: String::new(),
        item_tier: "",
        magic_quality: "",
        drop_chance: 0.0,
    };

    for (base_code, (leaf_prob, ratios)) in resolved.iter() {
        if *leaf_prob <= 0.0 {
            continue;
        }
        let Some(base_item) = items.base_items_by_code.get(base_code) else { continue };
        let ratio_row = items.item_ratio_for(base_item);

        let unique_candidates = eligible_named(
            items.unique_items_by_base_code.get(base_code),
            instance.level,
            &instance.monster_class_id,
        );
        let set_candidates =
            eligible_named(items.set_items_by_base_code.get(base_code), instance.level, &instance.monster_class_id);

        let split = quality::split_quality(
            instance.level,
            base_item,
            ratio_row,
            ratios,
            config.magic_find,
            !unique_candidates.is_empty(),
            !set_candidates.is_empty(),
        );

        emit_named(emit, &base_row, base_item, &unique_candidates, *leaf_prob * split.unique, "Unique");
        emit_named(emit, &base_row, base_item, &set_candidates, *leaf_prob * split.set, "Set");

        // Magic/Rare are deliberately not emitted as their own rows (procedurally-named, not useful
        // to filter on individually) — but they're still subtracted out of White above them, so White
        // correctly reflects "this base item, unenchanted" (e.g. still socketable for runewords).
        let base_name = items.base_item_display_name(base_item);
        push_generic(emit, &base_row, base_item, &base_name, *leaf_prob * split.white, "Base Item");
    }
}

fn emit_named(
    emit: &mut dyn FnMut(DropRow),
    base_row: &DropRow,
    base_item: &crate::data::items::BaseItem,
    candidates: &[&NamedItem],
    total_prob: f64,
    quality_label: &'static str,
) {
    if candidates.is_empty() || total_prob <= 0.0 {
        return;
    }
    let rarity_sum: i64 = candidates.iter().map(|c| c.rarity).sum();
    if rarity_sum <= 0 {
        return;
    }
    for candidate in candidates {
        let chance = total_prob * (candidate.rarity as f64 / rarity_sum as f64);
        if chance <= 0.0 {
            continue;
        }
        emit(DropRow {
            item_name: candidate.name.clone(),
            item_type: base_item.item_type_display.clone(),
            item_tier: base_item.tier.label(),
            magic_quality: quality_label,
            drop_chance: chance,
            ..clone_base(base_row)
        });
    }
}

fn push_generic(
    emit: &mut dyn FnMut(DropRow),
    base_row: &DropRow,
    base_item: &crate::data::items::BaseItem,
    base_name: &str,
    chance: f64,
    quality_label: &'static str,
) {
    if chance <= 0.0 {
        return;
    }
    emit(DropRow {
        item_name: base_name.to_string(),
        item_type: base_item.item_type_display.clone(),
        item_tier: base_item.tier.label(),
        magic_quality: quality_label,
        drop_chance: chance,
        ..clone_base(base_row)
    });
}

fn clone_base(base_row: &DropRow) -> DropRow {
    DropRow {
        monster_name: base_row.monster_name.clone(),
        monster_type: base_row.monster_type,
        monster_location: base_row.monster_location.clone(),
        act: base_row.act,
        difficulty: base_row.difficulty,
        terrorized: base_row.terrorized,
        res_fire: base_row.res_fire,
        res_cold: base_row.res_cold,
        res_lightning: base_row.res_lightning,
        res_poison: base_row.res_poison,
        res_magic: base_row.res_magic,
        res_physical: base_row.res_physical,
        item_name: base_row.item_name.clone(),
        item_tc: base_row.item_tc.clone(),
        item_type: base_row.item_type.clone(),
        item_tier: base_row.item_tier,
        magic_quality: base_row.magic_quality,
        drop_chance: base_row.drop_chance,
    }
}

