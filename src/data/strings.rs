use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct StringEntry {
    #[serde(rename = "Key")]
    key: String,
    #[serde(rename = "enUS")]
    en_us: String,
}

/// D2R's string-table JSON files are saved with a UTF-8 BOM, which serde_json doesn't strip.
fn strip_bom(data: &str) -> &str {
    data.strip_prefix('\u{FEFF}').unwrap_or(data)
}

pub fn load_string_table(data: &str) -> HashMap<String, String> {
    let entries: Vec<StringEntry> = serde_json::from_str(strip_bom(data)).expect("valid string-table json");
    entries.into_iter().map(|e| (e.key, e.en_us)).collect()
}
