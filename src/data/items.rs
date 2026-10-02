use super::raw::{first_csv_part, get, get_i32, get_i64, is_one, parse_tsv};
use crate::model::{ItemQuality, ItemTier};
use std::collections::{HashMap, HashSet};

pub struct ItemType {
    pub code: String,
    pub display_name: String,
    pub is_class_specific: bool,
    pub rarity: i64,
    /// All ancestor codes (via Equiv1/Equiv2, transitively) plus the code itself.
    pub ancestor_codes: HashSet<String>,
    pub can_be_rare: bool,
}

fn dfs_parents(code: &str, by_code: &HashMap<String, (String, String)>, out: &mut HashSet<String>) {
    if let Some((equiv1, equiv2)) = by_code.get(code) {
        for parent in [equiv1.as_str(), equiv2.as_str()] {
            if !parent.is_empty() && out.insert(parent.to_string()) {
                dfs_parents(parent, by_code, out);
            }
        }
    }
}

pub fn load_item_types(data: &str) -> HashMap<String, ItemType> {
    let rows = parse_tsv(data);
    let mut equiv_by_code: HashMap<String, (String, String)> = HashMap::new();
    for row in &rows {
        let code = get(row, "Code").to_string();
        if code.is_empty() {
            continue;
        }
        equiv_by_code.insert(code, (get(row, "Equiv1").to_string(), get(row, "Equiv2").to_string()));
    }

    let mut result = HashMap::new();
    for row in &rows {
        let code = get(row, "Code").to_string();
        if code.is_empty() {
            continue;
        }
        let mut ancestors = HashSet::new();
        dfs_parents(&code, &equiv_by_code, &mut ancestors);
        ancestors.insert(code.clone());
        let can_be_rare = is_one(row, "Rare");
        result.insert(
            code.clone(),
            ItemType {
                code,
                display_name: get(row, "ItemType").to_string(),
                is_class_specific: !get(row, "Class").trim().is_empty(),
                rarity: get_i64(row, "Rarity").unwrap_or(1),
                ancestor_codes: ancestors,
                can_be_rare,
            },
        );
    }
    result
}

#[derive(Clone)]
pub struct BaseItem {
    pub code: String,
    pub name_str: String,
    /// The raw `name` column — a fallback display label for the handful of items (e.g. gold) whose
    /// `namestr` isn't a real translation key with an entry in item-names.json.
    pub raw_name: String,
    pub item_type_code: String,
    pub item_type_display: String,
    pub item_type_rarity: i64,
    pub can_be_rare: bool,
    pub is_class_specific: bool,
    pub level: i32,
    pub tier: ItemTier,
    /// Virtual treasure-class bucket names this item belongs to, e.g. "weap3", "axe3", "mele3".
    pub virtual_tc_names: Vec<String>,
    /// Display-only stats for the web app's hover card.
    pub stats: BaseStats,
}

/// Base item stats as stored in weapons/armor/misc.txt; absent columns stay `None`.
#[derive(Clone, Default)]
pub struct BaseStats {
    pub required_level: Option<i32>,
    pub required_str: Option<i32>,
    pub required_dex: Option<i32>,
    pub defense: Option<(i32, i32)>,
    pub one_hand_damage: Option<(i32, i32)>,
    pub two_hand_damage: Option<(i32, i32)>,
    pub durability: Option<i32>,
    pub max_sockets: Option<i32>,
}

fn pair(row: &super::raw::Row, min: &str, max: &str) -> Option<(i32, i32)> {
    match (get_i32(row, min), get_i32(row, max)) {
        (Some(a), Some(b)) if a > 0 || b > 0 => Some((a, b)),
        _ => None,
    }
}

fn parse_base_stats(row: &super::raw::Row) -> BaseStats {
    let positive = |col: &str| get_i32(row, col).filter(|v| *v > 0);
    BaseStats {
        required_level: positive("levelreq"),
        required_str: positive("reqstr"),
        required_dex: positive("reqdex"),
        defense: pair(row, "minac", "maxac"),
        // Two-handed-only weapons (e.g. polearms) keep their damage in the 2H columns only.
        one_hand_damage: pair(row, "mindam", "maxdam"),
        two_hand_damage: pair(row, "2handmindam", "2handmaxdam"),
        durability: if is_one(row, "nodurability") { None } else { positive("durability") },
        max_sockets: positive("gemsockets"),
    }
}

/// One unique/set item property, as written in uniqueitems/setitems.txt (`prop`/`par`/`min`/`max`).
#[derive(Clone)]
pub struct RawProp {
    pub code: String,
    pub par: String,
    pub min: String,
    pub max: String,
}

fn read_props(row: &super::raw::Row, prop: &str, par: &str, min: &str, max: &str) -> Option<RawProp> {
    let code = get(row, prop).trim();
    if code.is_empty() {
        return None;
    }
    Some(RawProp {
        code: code.to_string(),
        par: get(row, par).trim().to_string(),
        min: get(row, min).trim().to_string(),
        max: get(row, max).trim().to_string(),
    })
}

fn parse_base_items(data: &str, item_types: &HashMap<String, ItemType>) -> Vec<BaseItem> {
    let rows = parse_tsv(data);
    let mut result = Vec::new();
    for row in &rows {
        let item_type_code = get(row, "type").to_string();
        if !is_one(row, "spawnable") || item_type_code == "ques" {
            continue;
        }
        let Some(item_type) = item_types.get(&item_type_code) else {
            continue;
        };
        let level = get_i32(row, "level").unwrap_or(0);
        let code = get(row, "code").to_string();
        let normcode = get(row, "normcode");
        let ubercode = get(row, "ubercode");
        let ultracode = get(row, "ultracode");
        let has_tier_family = !normcode.is_empty() || !ubercode.is_empty() || !ultracode.is_empty();
        // Thrown potions (Oil Potion, Rancid Gas Potion, Holy Water, ...) are technically weapons in
        // the game data (they deal missile damage, so they live in weapons.txt with real tier-family
        // columns) — checked ahead of the tier-family branch so they land as Consumable like every
        // other potion, not Normal/Exceptional/Elite.
        let is_potion = item_type_code == "tpot" || item_type.ancestor_codes.contains("poti");
        // Rings, amulets, jewels (incl. Colossal Jewel), charms, and the Torch base are real worn
        // equipment in misc.txt — they just have no normcode/ubercode/ultracode family because D2
        // never gave them Exceptional/Elite versions, so they'd otherwise fall through to Consumable.
        let is_equipment_misc = item_type_code == "ring"
            || item_type_code == "amul"
            || item_type_code == "torc"
            || item_type.ancestor_codes.contains("jewl")
            || item_type.ancestor_codes.contains("char");
        let tier = if item_type.ancestor_codes.contains("rune") {
            ItemTier::Rune
        } else if item_type.ancestor_codes.contains("gem") {
            // Covers all gem colors, the 5 gem-quality grades, and skulls (all share "gem" as an
            // itemtypes.txt ancestor).
            ItemTier::Gem
        } else if is_potion {
            ItemTier::Consumable
        } else if has_tier_family {
            // weapons.txt/armor.txt rows: every row belongs to a normal/exceptional/elite family
            // (even single-tier items just point all three codes at themselves).
            if code == normcode {
                ItemTier::Normal
            } else if code == ubercode {
                ItemTier::Exceptional
            } else if code == ultracode {
                ItemTier::Elite
            } else {
                ItemTier::Normal
            }
        } else if is_equipment_misc {
            ItemTier::Normal
        } else {
            // Everything else in misc.txt with no tier family and no gem/rune/potion/equipment
            // ancestry: gold, scrolls, tomes, keys, quivers (ammo), and similar.
            ItemTier::Consumable
        };
        let tc_level = level + (3 - level.rem_euclid(3)) % 3;
        let virtual_tc_names = item_type
            .ancestor_codes
            .iter()
            .map(|c| format!("{c}{tc_level}"))
            .collect();
        result.push(BaseItem {
            code,
            name_str: get(row, "namestr").trim().to_string(),
            raw_name: get(row, "name").trim().to_string(),
            item_type_code: item_type_code.clone(),
            item_type_display: item_type.display_name.clone(),
            item_type_rarity: item_type.rarity,
            can_be_rare: item_type.can_be_rare,
            is_class_specific: item_type.is_class_specific,
            level,
            tier,
            virtual_tc_names,
            stats: parse_base_stats(row),
        });
    }
    result
}

pub struct ItemQualityModifiers {
    pub ratio: i64,
    pub divisor: i64,
    pub min: i64,
}

pub struct ItemRatioRow {
    pub is_uber: bool,
    pub is_class_specific: bool,
    pub modifiers: HashMap<ItemQuality, ItemQualityModifiers>,
}

fn load_item_ratios(data: &str) -> Vec<ItemRatioRow> {
    let rows = parse_tsv(data);
    let mut result = Vec::new();
    for row in &rows {
        if get(row, "Version").trim() != "1" {
            continue;
        }
        let is_uber = is_one(row, "Uber");
        let is_class_specific = is_one(row, "Class Specific");
        let mut modifiers = HashMap::new();
        for (quality, prefix) in [
            (ItemQuality::Unique, "Unique"),
            (ItemQuality::Rare, "Rare"),
            (ItemQuality::Set, "Set"),
            (ItemQuality::Magic, "Magic"),
        ] {
            modifiers.insert(
                quality,
                ItemQualityModifiers {
                    ratio: get_i64(row, prefix).unwrap_or(0),
                    divisor: get_i64(row, &format!("{prefix}Divisor")).unwrap_or(1),
                    min: get_i64(row, &format!("{prefix}Min")).unwrap_or(0),
                },
            );
        }
        result.push(ItemRatioRow { is_uber, is_class_specific, modifiers });
    }
    result
}

#[derive(Clone)]
pub struct NamedItem {
    pub id: String,
    /// In-game (enUS) name; `id` is the internal key, which sometimes differs (e.g. "McAuley's Taboo"
    /// is shown as "Sander's Taboo").
    pub name: String,
    pub base_code: String,
    pub required_level: Option<i32>,
    /// Set name (setitems.txt `set`); empty for uniques.
    pub set_name: String,
    pub props: Vec<RawProp>,
    /// Partial set bonuses as (number of set items worn, property).
    pub set_bonuses: Vec<(i32, RawProp)>,
    pub level: i32,
    pub rarity: i64,
    pub only_drops_from_monster_class: Option<String>,
}

fn load_unique_items(data: &str, base_items_by_code: &HashMap<String, BaseItem>) -> Vec<NamedItem> {
    let rows = parse_tsv(data);
    let mut result = Vec::new();
    for row in &rows {
        let level = get_i32(row, "lvl").unwrap_or(0);
        if level == 0 {
            continue;
        }
        let spawnable = is_one(row, "spawnable");
        let has_drop_condition = !get(row, "DropConditionCalc").trim().is_empty();
        if !spawnable || has_drop_condition {
            // Only obtainable via a scripted/special mechanic, not a flat monster-drop chance.
            continue;
        }
        let base_code = get(row, "code").to_string();
        if !base_items_by_code.contains_key(&base_code) {
            continue;
        }
        result.push(NamedItem {
            id: get(row, "index").trim().to_string(),
            name: String::new(),
            base_code,
            required_level: get_i32(row, "lvl req"),
            set_name: String::new(),
            props: (1..=12)
                .filter_map(|i| read_props(row, &format!("prop{i}"), &format!("par{i}"), &format!("min{i}"), &format!("max{i}")))
                .collect(),
            set_bonuses: Vec::new(),
            level,
            rarity: get_i64(row, "rarity").unwrap_or(1),
            only_drops_from_monster_class: None,
        });
    }
    result
}

fn load_set_items(data: &str, base_items_by_code: &HashMap<String, BaseItem>) -> Vec<NamedItem> {
    let rows = parse_tsv(data);
    let mut result = Vec::new();
    for row in &rows {
        let level = get_i32(row, "lvl").unwrap_or(0);
        if level == 0 {
            continue;
        }
        let base_code = get(row, "item").to_string();
        if !base_items_by_code.contains_key(&base_code) {
            continue;
        }
        let only_drops_from_monster_class =
            if get(row, "set") == "Cow King's Leathers" { Some("hellbovine".to_string()) } else { None };
        result.push(NamedItem {
            id: get(row, "index").trim().to_string(),
            name: String::new(),
            base_code,
            required_level: get_i32(row, "lvl req"),
            set_name: get(row, "set").trim().to_string(),
            props: (1..=9)
                .filter_map(|i| read_props(row, &format!("prop{i}"), &format!("par{i}"), &format!("min{i}"), &format!("max{i}")))
                .collect(),
            // aprop1a/b apply with 2 items worn, aprop2a/b with 3, and so on.
            set_bonuses: (1..=5)
                .flat_map(|i| {
                    ["a", "b"].into_iter().filter_map(move |x| {
                        read_props(row, &format!("aprop{i}{x}"), &format!("apar{i}{x}"), &format!("amin{i}{x}"), &format!("amax{i}{x}"))
                            .map(|p| (i + 1, p))
                    })
                })
                .collect(),
            level,
            rarity: get_i64(row, "rarity").unwrap_or(1),
            only_drops_from_monster_class,
        });
    }
    result
}

pub struct ItemDatabase {
    pub base_items_by_code: HashMap<String, BaseItem>,
    pub item_ratios: Vec<ItemRatioRow>,
    pub unique_items_by_base_code: HashMap<String, Vec<NamedItem>>,
    pub set_items_by_base_code: HashMap<String, Vec<NamedItem>>,
    /// Virtual TC bucket name -> candidate (base item code, weight) pairs.
    pub virtual_treasure_classes: HashMap<String, Vec<(String, i64)>>,
    pub item_names: HashMap<String, String>,
}

impl ItemDatabase {
    pub fn load(
        weapons_txt: &str,
        armor_txt: &str,
        misc_txt: &str,
        itemtypes_txt: &str,
        itemratio_txt: &str,
        uniqueitems_txt: &str,
        setitems_txt: &str,
        item_names: HashMap<String, String>,
    ) -> Self {
        let item_types = load_item_types(itemtypes_txt);
        let mut base_items = Vec::new();
        base_items.extend(parse_base_items(weapons_txt, &item_types));
        base_items.extend(parse_base_items(armor_txt, &item_types));
        base_items.extend(parse_base_items(misc_txt, &item_types));

        let mut virtual_treasure_classes: HashMap<String, Vec<(String, i64)>> = HashMap::new();
        for item in &base_items {
            for tc_name in &item.virtual_tc_names {
                virtual_treasure_classes
                    .entry(tc_name.clone())
                    .or_default()
                    .push((item.code.clone(), item.item_type_rarity));
            }
        }

        let base_items_by_code: HashMap<String, BaseItem> =
            base_items.into_iter().map(|i| (i.code.clone(), i)).collect();

        let item_ratios = load_item_ratios(itemratio_txt);

        let named = |mut item: NamedItem| {
            item.name = item_names.get(&item.id).cloned().unwrap_or_else(|| item.id.clone());
            item
        };
        let mut unique_items_by_base_code: HashMap<String, Vec<NamedItem>> = HashMap::new();
        for item in load_unique_items(uniqueitems_txt, &base_items_by_code).into_iter().map(named) {
            unique_items_by_base_code.entry(item.base_code.clone()).or_default().push(item);
        }
        let mut set_items_by_base_code: HashMap<String, Vec<NamedItem>> = HashMap::new();
        for item in load_set_items(setitems_txt, &base_items_by_code).into_iter().map(named) {
            set_items_by_base_code.entry(item.base_code.clone()).or_default().push(item);
        }

        ItemDatabase {
            base_items_by_code,
            item_ratios,
            unique_items_by_base_code,
            set_items_by_base_code,
            virtual_treasure_classes,
            item_names,
        }
    }

    pub fn item_ratio_for(&self, base_item: &BaseItem) -> &ItemRatioRow {
        let is_uber = base_item.tier != ItemTier::Normal;
        self.item_ratios
            .iter()
            .find(|r| r.is_uber == is_uber && r.is_class_specific == base_item.is_class_specific)
            .expect("itemratio.txt should have a row for every (isUber, isClassSpecific) combination")
    }

    pub fn base_item_display_name(&self, base_item: &BaseItem) -> String {
        self.item_names
            .get(&base_item.name_str)
            .cloned()
            .unwrap_or_else(|| {
                if base_item.raw_name.is_empty() { base_item.name_str.clone() } else { base_item.raw_name.clone() }
            })
    }

    /// TC-referenced Item strings can carry a `code,modifier` annotation (e.g. a gold-quantity multiplier) —
    /// only the code itself matters for drop-chance purposes. The csv crate already strips the surrounding
    /// quotes these fields are written with in the raw txt file.
    pub fn normalize_leaf_code(raw: &str) -> String {
        first_csv_part(raw)
    }
}
