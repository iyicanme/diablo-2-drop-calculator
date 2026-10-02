use crate::data::items::ItemDatabase;
use crate::data::treasureclass::{TreasureClassLibrary, TreasureClassRow};
use crate::model::QualityRatios;
use std::collections::HashMap;

enum NodeKind<'a> {
    Defined(&'a TreasureClassRow),
    Virtual(&'a [(String, i64)], i64),
    Leaf,
    Unknown,
}

fn classify<'a>(name: &str, tcs: &'a TreasureClassLibrary, items: &'a ItemDatabase) -> NodeKind<'a> {
    if let Some(row) = tcs.by_name.get(name) {
        return NodeKind::Defined(row);
    }
    if let Some(entries) = items.virtual_treasure_classes.get(name) {
        let denom = entries.iter().map(|(_, w)| w).sum();
        return NodeKind::Virtual(entries, denom);
    }
    if items.base_items_by_code.contains_key(name) {
        return NodeKind::Leaf;
    }
    NodeKind::Unknown
}

/// Mirrors the D2/D2R "NoDrop" scaling with player count and party size: more players makes the
/// implicit "nothing" outcome proportionally less likely, asymptotically (never reaches zero).
fn calculate_no_drop(tc_denominator: i64, no_drop: Option<i64>, n_players: i32, party_size: i32) -> i64 {
    let Some(no_drop) = no_drop else { return 0 };
    if no_drop < 1 {
        return 0;
    }
    if n_players <= 1 {
        return no_drop;
    }
    let no_drop_exponent = (1.0 + (n_players as f64 - 1.0) / 2.0 + (party_size as f64 - 1.0) / 2.0).floor();
    let base_no_drop_rate = no_drop as f64 / (no_drop + tc_denominator) as f64;
    let new_no_drop_rate = base_no_drop_rate.powf(no_drop_exponent);
    let new_no_drop_num = (new_no_drop_rate / (1.0 - new_no_drop_rate)) * tc_denominator as f64;
    new_no_drop_num.floor() as i64
}

struct LeafAccum {
    probability: f64,
    ratios: QualityRatios,
    picks: i64,
}

type Bucket = HashMap<String, LeafAccum>;

#[allow(clippy::too_many_arguments)]
fn recurse(
    name: &str,
    sel_num: i64,
    sel_den: i64,
    parent_picks: i32,
    path_probability: f64,
    ratios_acc: QualityRatios,
    buckets: &mut Vec<Bucket>,
    n_players: i32,
    party_size: i32,
    accumulated_picks: i64,
    tcs: &TreasureClassLibrary,
    items: &ItemDatabase,
) {
    let kind = classify(name, tcs, items);
    let configured_picks: i32 = match &kind {
        NodeKind::Defined(row) => row.picks,
        _ => 1,
    };
    let parent_picks_negative = parent_picks < 0;
    let adjusted_picks: i64 = if configured_picks < 0 { accumulated_picks } else { configured_picks as i64 };
    let updated_accumulated_picks: i64 =
        if parent_picks < 0 { sel_num * accumulated_picks * adjusted_picks } else { accumulated_picks * adjusted_picks };
    let selection_probability =
        if parent_picks_negative { 1.0 } else { (sel_num as f64 / sel_den as f64) * path_probability };

    if parent_picks_negative && buckets.last().map(|b| !b.is_empty()).unwrap_or(false) {
        buckets.push(HashMap::new());
    }

    let node_ratios = match &kind {
        NodeKind::Defined(row) => ratios_acc.merge(&row.ratios),
        _ => ratios_acc,
    };

    match kind {
        NodeKind::Leaf => {
            let bucket = buckets.last_mut().expect("at least one bucket always exists");
            let entry = bucket.entry(name.to_string()).or_insert(LeafAccum {
                probability: 0.0,
                ratios: QualityRatios::default(),
                picks: updated_accumulated_picks,
            });
            entry.probability += selection_probability;
            entry.ratios = entry.ratios.merge(&node_ratios);
            entry.picks = updated_accumulated_picks;
        }
        NodeKind::Defined(row) => {
            let denom = row.items_denominator();
            let denom_with_no_drop = denom + calculate_no_drop(denom, row.no_drop, n_players, party_size);
            for (item_name, weight) in &row.items {
                recurse(
                    item_name,
                    *weight,
                    denom_with_no_drop,
                    configured_picks,
                    selection_probability,
                    node_ratios,
                    buckets,
                    n_players,
                    party_size,
                    updated_accumulated_picks,
                    tcs,
                    items,
                );
            }
        }
        NodeKind::Virtual(entries, denom) => {
            for (item_code, weight) in entries {
                recurse(
                    item_code,
                    *weight,
                    denom,
                    configured_picks,
                    selection_probability,
                    node_ratios,
                    buckets,
                    n_players,
                    party_size,
                    updated_accumulated_picks,
                    tcs,
                    items,
                );
            }
        }
        NodeKind::Unknown => {}
    }
}

fn apply_picks(probability: f64, picks: i64) -> f64 {
    let picks = picks.max(1).min(6);
    if picks <= 1 {
        probability
    } else {
        1.0 - (1.0 - probability).powi(picks as i32)
    }
}

/// Resolves a treasure class name all the way down to concrete base-item codes, returning for each
/// the chance of at least one drop of that base item plus the merged Unique/Set/Rare/Magic quality
/// ratios accumulated along the path(s) that reach it.
pub fn resolve_to_base_items(
    tcs: &TreasureClassLibrary,
    items: &ItemDatabase,
    tc_name: &str,
    n_players: i32,
    party_size: i32,
) -> HashMap<String, (f64, QualityRatios)> {
    let root_ratios = tcs.by_name.get(tc_name).map(|r| r.ratios).unwrap_or_default();
    let mut buckets: Vec<Bucket> = vec![HashMap::new()];
    recurse(tc_name, 1, 1, 1, 1.0, root_ratios, &mut buckets, n_players, party_size, 1, tcs, items);

    let mut per_leaf: HashMap<String, Vec<(f64, QualityRatios)>> = HashMap::new();
    for bucket in &buckets {
        for (leaf, accum) in bucket {
            let p = apply_picks(accum.probability, accum.picks);
            per_leaf.entry(leaf.clone()).or_default().push((p, accum.ratios));
        }
    }

    per_leaf
        .into_iter()
        .map(|(leaf, paths)| {
            let combined_prob = 1.0 - paths.iter().map(|(p, _)| 1.0 - p).product::<f64>();
            let combined_ratios =
                paths.iter().fold(QualityRatios::default(), |acc, (_, r)| acc.merge(r));
            (leaf, (combined_prob, combined_ratios))
        })
        .collect()
}
