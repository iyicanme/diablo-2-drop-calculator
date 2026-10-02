//! Hover-card details for the web app: one JSON document describing every monster, area and item
//! the drop rows reference (by the keys in `DropRow::monster_key` / `area_id` / `item_key`).

use crate::data::items::{BaseItem, NamedItem};
use crate::data::{DisplayData, GameData};
use crate::model::{Difficulty, MonsterType};
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};

fn per_difficulty(mut f: impl FnMut(Difficulty) -> Option<Value>) -> Value {
    let mut out = Map::new();
    for d in Difficulty::ALL {
        if let Some(v) = f(d) {
            out.insert(d.label().to_string(), v);
        }
    }
    Value::Object(out)
}

fn monster(game_data: &GameData, key: &str) -> Value {
    let (name, class_id, superunique) = match key.strip_prefix("su:") {
        Some(id) => match game_data.superuniques.iter().find(|s| s.id == id) {
            Some(s) => (s.name.clone(), s.monster_class_id.as_str(), true),
            None => return Value::Null,
        },
        None => (String::new(), key, false),
    };
    let Some(class) = game_data.monster_classes.get(class_id) else { return Value::Null };
    let class_name = game_data.monster_display_name(&class.name_str);
    json!({
        "name": if superunique { name } else { class_name.clone() },
        // A superunique is a named variant of an ordinary monster class.
        "base": if superunique { Value::String(class_name) } else { Value::Null },
        "undead": class.is_undead,
        "demon": class.is_demon,
        "boss": class.is_boss,
        "stats": per_difficulty(|d| {
            let s = class.stats.get(&d)?;
            let r = class.resistances.get(&d).copied().unwrap_or_default();
            Some(json!({
                "hp": [s.min_hp_pct, s.max_hp_pct],
                "xp": s.exp_pct,
                // Same order as the immunity checkboxes.
                "res": [r.fire, r.cold, r.lightning, r.poison, r.magic, r.physical],
            }))
        }),
    })
}

fn area(game_data: &GameData, id: &str) -> Value {
    let Some(area) = game_data.areas.get(id) else { return Value::Null };
    let superuniques: Vec<&str> =
        game_data.superuniques.iter().filter(|s| s.area_id == id).map(|s| s.name.as_str()).collect();
    json!({
        "name": area.display_name,
        "act": area.act,
        "levels": per_difficulty(|d| area.monster_levels.get(&d).map(|l| json!(l))),
        "uniquePacks": per_difficulty(|d| area.unique_packs.get(&d).map(|(a, b)| json!([a, b]))),
        "monsters": per_difficulty(|d| {
            let ids = area.monster_class_ids.get(&(d, MonsterType::Regular))?;
            let names: BTreeSet<String> = ids
                .iter()
                .filter_map(|id| game_data.monster_classes.get(id))
                .map(|c| game_data.monster_display_name(&c.name_str))
                .collect();
            Some(json!(names))
        }),
        "superuniques": superuniques,
    })
}

fn base_stats(game_data: &GameData, base: &BaseItem) -> Map<String, Value> {
    let s = &base.stats;
    let mut out = Map::new();
    out.insert("base".into(), json!(game_data.items.base_item_display_name(base)));
    out.insert("type".into(), json!(base.item_type_display));
    out.insert("tier".into(), json!(base.tier.label()));
    out.insert("qlvl".into(), json!(base.level));
    let mut put = |k: &str, v: Option<Value>| {
        if let Some(v) = v {
            out.insert(k.to_string(), v);
        }
    };
    put("reqLevel", s.required_level.map(|v| json!(v)));
    put("reqStr", s.required_str.map(|v| json!(v)));
    put("reqDex", s.required_dex.map(|v| json!(v)));
    put("defense", s.defense.map(|(a, b)| json!([a, b])));
    put("damage1h", s.one_hand_damage.map(|(a, b)| json!([a, b])));
    put("damage2h", s.two_hand_damage.map(|(a, b)| json!([a, b])));
    put("durability", s.durability.map(|v| json!(v)));
    put("sockets", s.max_sockets.map(|v| json!(v)));
    out
}

fn item(game_data: &GameData, display: &DisplayData, named: &HashMap<&str, (&NamedItem, &str)>, key: &str) -> Value {
    let items = &game_data.items;
    if let Some(code) = key.strip_prefix("b:") {
        let Some(base) = items.base_items_by_code.get(code) else { return Value::Null };
        let mut out = base_stats(game_data, base);
        out.insert("name".into(), json!(items.base_item_display_name(base)));
        out.insert("kind".into(), json!("base"));
        // Runes and gems: what they add when socketed, per kind of item.
        if let Some(slots) = display.socket_bonuses.get(code) {
            let socketed: Vec<Value> = slots
                .iter()
                .map(|(slot, props)| {
                    let lines: Vec<String> = props.iter().filter_map(|p| display.props.render(p)).collect();
                    json!({ "slot": slot, "lines": lines })
                })
                .filter(|s| s["lines"].as_array().is_some_and(|l| !l.is_empty()))
                .collect();
            out.insert("socketed".into(), json!(socketed));
        }
        return Value::Object(out);
    }
    let Some(&(n, kind)) = named.get(key) else { return Value::Null };
    let Some(base) = items.base_items_by_code.get(&n.base_code) else { return Value::Null };
    let mut out = base_stats(game_data, base);
    out.insert("name".into(), json!(n.name));
    out.insert("kind".into(), json!(kind));
    // A named item's own qlvl / level requirement replace the base item's.
    out.insert("qlvl".into(), json!(n.level));
    if let Some(r) = n.required_level {
        out.insert("reqLevel".into(), json!(r));
    }
    if !n.set_name.is_empty() {
        out.insert("set".into(), json!(n.set_name));
    }
    let props: Vec<String> = n.props.iter().filter_map(|p| display.props.render(p)).collect();
    out.insert("props".into(), json!(props));
    let bonuses: Vec<Value> = n
        .set_bonuses
        .iter()
        .filter_map(|(count, p)| Some(json!({ "items": count, "text": display.props.render(p)? })))
        .collect();
    if !bonuses.is_empty() {
        out.insert("setBonuses".into(), json!(bonuses));
    }
    Value::Object(out)
}

/// Details for exactly the given keys, each list in index order (row columns refer to positions).
pub fn build(
    game_data: &GameData,
    display: &DisplayData,
    monster_keys: &[String],
    area_ids: &[String],
    item_keys: &[String],
) -> Value {
    let mut named: HashMap<&str, (&NamedItem, &str)> = HashMap::new();
    let keyed: Vec<(String, &NamedItem, &str)> = game_data
        .items
        .unique_items_by_base_code
        .values()
        .flatten()
        .map(|n| (format!("u:{}", n.id), n, "unique"))
        .chain(game_data.items.set_items_by_base_code.values().flatten().map(|n| (format!("s:{}", n.id), n, "set")))
        .collect();
    for (key, n, kind) in &keyed {
        named.insert(key.as_str(), (n, kind));
    }

    let levels = &display.monster_levels;
    json!({
        "monlvl": per_difficulty(|d| Some(json!({ "hp": levels.hp.get(&d), "xp": levels.xp.get(&d) }))),
        "monsters": monster_keys.iter().map(|k| monster(game_data, k)).collect::<Vec<_>>(),
        "areas": area_ids.iter().map(|id| area(game_data, id)).collect::<Vec<_>>(),
        "items": item_keys.iter().map(|k| item(game_data, display, &named, k)).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named_props(game_data: &GameData, display: &DisplayData, id: &str) -> Vec<String> {
        let n = game_data
            .items
            .unique_items_by_base_code
            .values()
            .chain(game_data.items.set_items_by_base_code.values())
            .flatten()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("{id} not found"));
        n.props.iter().filter_map(|p| display.props.render(p)).collect()
    }

    #[test]
    fn renders_unique_props() {
        let (g, d) = (GameData::load(), DisplayData::load());
        let shako = named_props(&g, &d, "Harlequin Crest");
        for line in ["+2 to All Skills", "+(1.5 per Level) to Life (Based on Character Level)", "50% Better Chance of Getting Magic Items"] {
            assert!(shako.iter().any(|l| l == line), "{line:?} not in {shako:?}");
        }
        let griffon = named_props(&g, &d, "Griffon's Eye");
        assert!(griffon.iter().any(|l| l == "+(10-15)% to Lightning Skill Damage"), "{griffon:?}");
        let carin = named_props(&g, &d, "Carin Shard");
        assert!(carin.iter().any(|l| l == "+2 to Summoning Skills (Necromancer only)"), "{carin:?}");
    }

    #[test]
    fn renders_socket_bonuses() {
        let d = DisplayData::load();
        let lines = |code: &str, slot: &str| -> Vec<String> {
            let slots = &d.socket_bonuses[code];
            let (_, props) = slots.iter().find(|(s, _)| *s == slot).unwrap();
            props.iter().filter_map(|p| d.props.render(p)).collect()
        };
        assert_eq!(lines("gcb", "Armor and helms"), ["+10 to Mana"]);
        assert_eq!(lines("r30", "Weapons"), ["20% Chance of Crushing Blow"]);
        assert_eq!(lines("r33", "Shields"), ["Indestructible"]);
    }

    #[test]
    fn monster_life_matches_known_values() {
        let (g, d) = (GameData::load(), DisplayData::load());
        let life = |id: &str, diff: Difficulty, level: usize| {
            let s = g.monster_classes[id].stats[&diff];
            let base = d.monster_levels.hp[&diff][level];
            (s.min_hp_pct as i64 * base / 100, s.max_hp_pct as i64 * base / 100)
        };
        assert_eq!(life("fallen1", Difficulty::Normal, 1), (1, 4));
        // Published single-player values (e.g. diablo2.wiki.fextralife.com, rankedboost.com).
        assert_eq!(life("andariel", Difficulty::Normal, 12).0, 1024);
        assert_eq!(life("diablo", Difficulty::Normal, 40).0, 13818);
    }
}
