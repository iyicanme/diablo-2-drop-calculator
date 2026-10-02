//! Turns unique/set item property codes (`allskills 2`, `hit-skill 45 10/10`, ...) into readable
//! lines for the web app's hover card.
//!
//! Wording comes from properties.json's `*Tooltip` templates (a designer column, close to but not
//! always identical to the in-game text): each `#` is filled from `min`/`max`/`par`, interpreted
//! per the property's `func1` the same way the file's own `*Min`/`*Max`/`*Parameter` notes describe.

use super::items::RawProp;
use serde_json::Value;
use std::collections::HashMap;

const CLASS_NAMES: [(&str, &str); 8] = [
    ("ama", "Amazon"),
    ("sor", "Sorceress"),
    ("nec", "Necromancer"),
    ("pal", "Paladin"),
    ("bar", "Barbarian"),
    ("dru", "Druid"),
    ("ass", "Assassin"),
    ("war", "Warlock"),
];

/// `skilltab` parameter -> tab name; three tabs per class in CLASS_NAMES order (properties.json's
/// notes: Amazon = 0-2, Sorceress = 3-5, ... Assassin = 18-20).
const SKILL_TABS: [&str; 21] = [
    "Bow and Crossbow Skills",
    "Passive and Magic Skills",
    "Javelin and Spear Skills",
    "Fire Skills",
    "Lightning Skills",
    "Cold Skills",
    "Curses",
    "Poison and Bone Skills",
    "Summoning Skills",
    "Combat Skills",
    "Offensive Auras",
    "Defensive Auras",
    "Combat Skills",
    "Masteries",
    "Warcries",
    "Summoning Skills",
    "Shape Shifting Skills",
    "Elemental Skills",
    "Traps",
    "Shadow Disciplines",
    "Martial Arts",
];

/// Modifiers of a neighbouring damage property (e.g. poison duration), not lines of their own.
const SKIPPED: [&str; 3] = ["pois-len", "cold-len", "bloody"];

struct Skill {
    name: String,
    class: Option<&'static str>,
}

pub struct PropRenderer {
    templates: HashMap<String, (i64, String)>,
    skills_by_id: HashMap<i64, Skill>,
    skill_ids_by_name: HashMap<String, i64>,
}

fn strip_bom(data: &str) -> &str {
    data.strip_prefix('\u{FEFF}').unwrap_or(data)
}

fn int(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

/// "5", or "5-10" when the roll varies ("-65 to -45" for negative rolls).
fn range(min: i64, max: i64) -> String {
    if min == max {
        min.to_string()
    } else if min < 0 || max < 0 {
        format!("{min} to {max}")
    } else {
        format!("{min}-{max}")
    }
}

/// Up to two decimals, without trailing zeros: 1.5, 0.125 -> 0.13, 2.
fn decimal(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl PropRenderer {
    pub fn load(properties_json: &str, skills_json: &str, skilldesc_json: &str, strings_json: &str) -> Self {
        let properties: HashMap<String, Value> = serde_json::from_str(strip_bom(properties_json)).expect("properties.json");
        let skills: HashMap<String, Value> = serde_json::from_str(strip_bom(skills_json)).expect("skills.json");
        let skilldesc: HashMap<String, Value> = serde_json::from_str(strip_bom(skilldesc_json)).expect("skilldesc.json");
        let strings: HashMap<String, String> = serde_json::from_str(strip_bom(strings_json)).expect("localestrings-eng.json");

        let templates = properties
            .values()
            .filter_map(|p| {
                let code = p.get("code")?.as_str()?;
                let tooltip = p.get("*Tooltip")?.as_str()?;
                Some((code.to_string(), (p.get("func1").and_then(Value::as_i64).unwrap_or(1), tooltip.to_string())))
            })
            .collect();

        let name_key_by_desc: HashMap<&str, &str> = skilldesc
            .values()
            .filter_map(|d| Some((d.get("skilldesc")?.as_str()?, d.get("str name")?.as_str()?)))
            .collect();
        let mut skills_by_id = HashMap::new();
        let mut skill_ids_by_name = HashMap::new();
        for s in skills.values() {
            let (Some(id), Some(raw)) = (s.get("*Id").and_then(Value::as_i64), s.get("skill").and_then(Value::as_str)) else {
                continue;
            };
            let name = s
                .get("skilldesc")
                .and_then(Value::as_str)
                .and_then(|d| name_key_by_desc.get(d))
                .and_then(|k| strings.get(*k))
                .cloned()
                .unwrap_or_else(|| raw.to_string());
            let class = s
                .get("charclass")
                .and_then(Value::as_str)
                .and_then(|c| CLASS_NAMES.iter().find(|(code, _)| *code == c).map(|(_, n)| *n));
            skill_ids_by_name.insert(raw.to_lowercase(), id);
            skills_by_id.insert(id, Skill { name, class });
        }
        PropRenderer { templates, skills_by_id, skill_ids_by_name }
    }

    fn skill(&self, par: &str) -> Option<&Skill> {
        let id = int(par).or_else(|| self.skill_ids_by_name.get(&par.to_lowercase()).copied())?;
        self.skills_by_id.get(&id)
    }

    /// One readable line, or `None` for properties that don't stand on their own.
    pub fn render(&self, p: &RawProp) -> Option<String> {
        if p.code.starts_with('*') || SKIPPED.contains(&p.code.as_str()) {
            return None;
        }
        let (min, max, par) = (int(&p.min), int(&p.max), int(&p.par));
        let value = match (min, max) {
            (Some(a), Some(b)) => Some(range(a, b)),
            (Some(a), None) | (None, Some(a)) => Some(a.to_string()),
            (None, None) => par.map(|v| v.to_string()),
        };

        // Properties whose template is missing, wrong (pierce-mag says "Fire") or doesn't fit the
        // parameter layout.
        // A range after a sign reads better in parentheses: "-(10-20)%".
        let signed = value.as_ref().map(|v| if v.contains('-') { format!("({v})") } else { v.clone() });
        match p.code.as_str() {
            "res-all-max" => return Some(format!("+{}% to All Maximum Resistances", signed?)),
            "dur" => return Some(format!("+{} Maximum Durability", signed?)),
            "pierce-mag" => return Some(format!("-{}% to Enemy Magic Resistance", signed?)),
            // par = repair speed: 1 durability every 100/par seconds.
            "rep-dur" => return Some(format!("Repairs 1 Durability in {} Seconds", decimal(100.0 / par.filter(|v| *v > 0)? as f64))),
            "randclassskill" => return Some("+ to Random Class Skill Levels (class rolled on drop)".to_string()),
            "skill-rand" => {
                let class = min.and_then(|id| self.skills_by_id.get(&id)).and_then(|s| s.class);
                return Some(match class {
                    Some(c) => format!("+{} to a Random {c} Skill", par?),
                    None => format!("+{} to a Random Skill", par?),
                });
            }
            "skilltab" => {
                let tab = par.filter(|t| (0..21).contains(t))? as usize;
                return Some(format!("+{} to {} ({} only)", signed?, SKILL_TABS[tab], CLASS_NAMES[tab / 3].1));
            }
            _ => {}
        }

        let Some((func, template)) = self.templates.get(&p.code) else {
            return Some(format!("{} {}", p.code, value.or(Some(p.par.clone())).unwrap_or_default()).trim().to_string());
        };
        let skill = self.skill(&p.par);
        let skill_name = skill.map(|s| s.name.clone()).unwrap_or_else(|| p.par.clone());

        // Values for the template's `#` placeholders, in order.
        let mut text = template.clone();
        let values: Vec<String> = match func {
            // Chance-to-cast: min = chance (0 means the default 5%), max = skill level.
            11 => vec![min.filter(|c| *c > 0).unwrap_or(5).to_string(), max.unwrap_or(1).to_string()],
            // Charges: max = skill level, min = charges.
            19 => {
                let charges = min.unwrap_or(0).to_string();
                vec![max.unwrap_or(1).to_string(), charges.clone(), charges]
            }
            // Adds min-max damage (+ duration in frames for poison/cold).
            15 => {
                let (a, b) = (min.unwrap_or(0), max.unwrap_or(0));
                let frames = par.unwrap_or(0);
                // Poison's per-frame value is in 256ths and applied every frame of the duration.
                let (a, b) = if p.code == "dmg-pois" {
                    ((a * frames + 128) / 256, (b * frames + 128) / 256)
                } else {
                    (a, b)
                };
                if a == b {
                    text = text.replacen("#-#", "#", 1);
                    vec![a.to_string(), decimal(frames as f64 / 25.0)]
                } else {
                    vec![a.to_string(), b.to_string(), decimal(frames as f64 / 25.0)]
                }
            }
            // Per character level: par is in 8ths per level.
            17 => vec![format!("({} per Level)", decimal(par.unwrap_or(0) as f64 / 8.0))],
            _ => vec![value.clone().unwrap_or_default()],
        };

        let mut out = String::new();
        let mut values = values.into_iter();
        let mut last = None;
        for (i, part) in text.split('#').enumerate() {
            if i > 0 {
                let v = values.next().or_else(|| last.clone()).unwrap_or_default();
                let signed = out.ends_with('+') || out.ends_with('-');
                if v.starts_with('-') && signed {
                    // "+#" / "-#" templates with a negative value: let the value carry the sign.
                    out.pop();
                    out.push_str(&v);
                } else if signed && v.contains('-') {
                    // "+(150-180)%" rather than the ambiguous "-7-15%".
                    out.push_str(&format!("({v})"));
                } else {
                    out.push_str(&v);
                }
                last = Some(v);
            }
            out.push_str(part);
        }
        let class = skill.and_then(|s| s.class).unwrap_or("");
        let out = out
            .replace("[Skill]", &skill_name)
            .replace("[Class]", class)
            .replace(" ( only)", "")
            .replace("[Returned]", "Returned");
        Some(out.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}
