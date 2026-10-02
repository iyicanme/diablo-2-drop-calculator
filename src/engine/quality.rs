use crate::data::items::{BaseItem, ItemQualityModifiers};
use crate::model::{ItemQuality, QualityRatios};

fn effective_mf_int(magic_find: i64, factor: i64) -> i64 {
    if magic_find <= 10 { magic_find } else { magic_find * factor / (magic_find + factor) }
}

fn effective_mf(magic_find: i64, quality: ItemQuality) -> i64 {
    match quality {
        ItemQuality::Unique => effective_mf_int(magic_find, 250),
        ItemQuality::Set => effective_mf_int(magic_find, 500),
        ItemQuality::Rare => effective_mf_int(magic_find, 600),
        ItemQuality::Magic => magic_find,
        ItemQuality::White => 0,
    }
}

/// The classic D2 magic-find formula: `ratio`/`divisor`/`min` come from itemratio.txt, and everything
/// is deliberately kept as truncating integer arithmetic (matching the original engine) until the very
/// last step, where it's expressed as the fraction 128/chanceAfterFactor.
fn prob_of_single_quality(
    quality: ItemQuality,
    monster_level: i32,
    base_item: &BaseItem,
    modifiers: &ItemQualityModifiers,
    ratios: &QualityRatios,
    magic_find: i64,
) -> f64 {
    let level_diff = (monster_level - base_item.level) as i64;
    let chance = modifiers.ratio - (level_diff / modifiers.divisor);
    let mul_chance = chance * 128;
    let mf = effective_mf(magic_find, quality);
    let chance_with_mf = (mul_chance * 100) / (100 + mf);
    let chance_after_min = modifiers.min.max(chance_with_mf);
    let chance_after_factor = chance_after_min - (chance_after_min * ratios.get(quality) / 1024);
    if chance_after_factor <= 0 { 1.0 } else { 128.0 / chance_after_factor as f64 }
}

/// A single quality's contribution, and the running "nothing higher-tier happened" survival factor.
pub struct QualitySplit {
    pub unique: f64,
    pub set: f64,
    pub rare: f64,
    pub magic: f64,
    pub white: f64,
}

/// Splits the chance of "this base item drops" into White/Magic/Rare/Set/Unique, sequentially,
/// highest tier first — each lower tier only gets a shot at the probability mass the tiers above it
/// didn't already claim. White is deliberately computed as the true remainder (not hardcoded to 1),
/// unlike some existing calculators, so the five outcomes are a real probability distribution.
pub fn split_quality(
    monster_level: i32,
    base_item: &BaseItem,
    ratio_row: &crate::data::items::ItemRatioRow,
    ratios: &QualityRatios,
    magic_find: i64,
    unique_eligible: bool,
    set_eligible: bool,
) -> QualitySplit {
    let get = |q: ItemQuality| -> f64 {
        let modifiers = ratio_row.modifiers.get(&q).expect("itemratio.txt covers all four qualities");
        prob_of_single_quality(q, monster_level, base_item, modifiers, ratios, magic_find)
    };

    let p_unique = if unique_eligible { get(ItemQuality::Unique) } else { 0.0 };
    let survive_unique = 1.0 - p_unique;

    let p_set = if set_eligible { survive_unique * get(ItemQuality::Set) } else { 0.0 };
    let survive_set = survive_unique * (1.0 - if set_eligible { get(ItemQuality::Set) } else { 0.0 });

    let (p_rare, p_magic, p_white) = if base_item.can_be_rare {
        let p_rare = survive_set * get(ItemQuality::Rare);
        let survive_rare = survive_set * (1.0 - get(ItemQuality::Rare));
        let p_magic = survive_rare * get(ItemQuality::Magic);
        let survive_magic = survive_rare * (1.0 - get(ItemQuality::Magic));
        (p_rare, p_magic, survive_magic)
    } else {
        (0.0, 0.0, survive_set)
    };

    QualitySplit { unique: p_unique, set: p_set, rare: p_rare, magic: p_magic, white: p_white }
}
