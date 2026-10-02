use anyhow::Result;
use std::path::Path;

pub struct DropRow {
    pub monster_name: String,
    pub monster_type: &'static str,
    pub monster_location: String,
    pub act: i32,
    pub difficulty: &'static str,
    pub terrorized: bool,
    pub res_fire: bool,
    pub res_cold: bool,
    pub res_lightning: bool,
    pub res_poison: bool,
    pub res_magic: bool,
    pub res_physical: bool,
    pub item_name: String,
    pub item_tc: String,
    pub item_type: String,
    pub item_tier: &'static str,
    pub magic_quality: &'static str,
    pub drop_chance: f64,
    // Not written to the CSV: references the web app uses to look up hover-card details.
    /// Monster class id, or `su:<superunique id>`.
    pub monster_key: String,
    pub area_id: String,
    /// `u:<unique id>`, `s:<set item id>` or `b:<base item code>`.
    pub item_key: String,
    pub monster_level: i32,
}

const HEADER: [&str; 19] = [
    "Monster",
    "Monster Type",
    "Location",
    "Act",
    "Difficulty",
    "Terrorized",
    "Fire Immune",
    "Cold Immune",
    "Lightning Immune",
    "Poison Immune",
    "Magic Immune",
    "Physical Immune",
    "Item",
    "Treasure Class",
    "Item Type",
    "Tier",
    "Quality",
    "Drop Chance",
    "Chance (1 in X)",
];

/// The three-way split the CSV is broken into: named Unique/Set items always win their own bucket
/// regardless of tier; everything else is split by whether it's equipment (a base item) or a
/// consumable/rune/gem.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    BaseItems,
    ConsumablesRunesGems,
    UniquesSets,
}

impl Category {
    pub fn of(row: &DropRow) -> Category {
        if row.magic_quality == "Unique" || row.magic_quality == "Set" {
            Category::UniquesSets
        } else if matches!(row.item_tier, "Consumable" | "Rune" | "Gem") {
            Category::ConsumablesRunesGems
        } else {
            Category::BaseItems
        }
    }

    fn file_name(&self) -> &'static str {
        match self {
            Category::BaseItems => "base_items.csv",
            Category::ConsumablesRunesGems => "consumables_runes_gems.csv",
            Category::UniquesSets => "uniques_sets.csv",
        }
    }
}

/// `chance_per_x` for a row (1/drop_chance) — "1 in X" is much easier to read than a tiny decimal.
pub fn chance_per_x(row: &DropRow) -> Option<f64> {
    if row.drop_chance > 0.0 { Some(1.0 / row.drop_chance) } else { None }
}

pub fn write_csvs_by_category(
    output_dir: &Path,
    rows: &[DropRow],
    max_chance_per_x: Option<f64>,
) -> Result<Vec<(&'static str, usize)>> {
    std::fs::create_dir_all(output_dir)?;
    let mut groups: std::collections::BTreeMap<Category, Vec<&DropRow>> = std::collections::BTreeMap::new();
    for row in rows {
        if let Some(max) = max_chance_per_x {
            if chance_per_x(row).is_none_or(|x| x > max) {
                continue;
            }
        }
        groups.entry(Category::of(row)).or_default().push(row);
    }
    let mut summary = Vec::new();
    for (category, group_rows) in groups {
        let path = output_dir.join(category.file_name());
        let mut wtr = csv::Writer::from_path(&path)?;
        wtr.write_record(HEADER)?;
        for row in &group_rows {
            write_row(&mut wtr, row)?;
        }
        wtr.flush()?;
        summary.push((category.file_name(), group_rows.len()));
    }
    Ok(summary)
}

fn write_row(wtr: &mut csv::Writer<std::fs::File>, row: &DropRow) -> Result<()> {
    let chance_per_x_str = chance_per_x(row).map(|x| format!("{x:.1}")).unwrap_or_default();
    wtr.write_record([
        row.monster_name.as_str(),
        row.monster_type,
        row.monster_location.as_str(),
        &row.act.to_string(),
        row.difficulty,
        yes_no(row.terrorized),
        yes_no(row.res_fire),
        yes_no(row.res_cold),
        yes_no(row.res_lightning),
        yes_no(row.res_poison),
        yes_no(row.res_magic),
        yes_no(row.res_physical),
        row.item_name.as_str(),
        row.item_tc.as_str(),
        row.item_type.as_str(),
        row.item_tier,
        row.magic_quality,
        &format!("{:.8}", row.drop_chance),
        &chance_per_x_str,
    ])?;
    Ok(())
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}
