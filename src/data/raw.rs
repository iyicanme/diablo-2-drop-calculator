use std::collections::HashMap;

pub type Row = HashMap<String, String>;

/// Parses a tab-separated D2 data file (with a header row) into a list of column->value maps.
/// Uses the `csv` crate so that quoted fields (e.g. `"gld,mul=2048"`) are handled correctly.
pub fn parse_tsv(data: &str) -> Vec<Row> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .flexible(true)
        .has_headers(true)
        .from_reader(data.as_bytes());
    let headers: Vec<String> = rdr.headers().unwrap().iter().map(|s| s.to_string()).collect();
    rdr.records()
        .filter_map(|r| r.ok())
        .map(|record| {
            let mut row = HashMap::with_capacity(headers.len());
            for (i, h) in headers.iter().enumerate() {
                row.insert(h.clone(), record.get(i).unwrap_or("").to_string());
            }
            row
        })
        .collect()
}

pub fn get<'a>(row: &'a Row, col: &str) -> &'a str {
    row.get(col).map(|s| s.as_str()).unwrap_or("")
}

pub fn get_i64(row: &Row, col: &str) -> Option<i64> {
    let v = get(row, col).trim();
    if v.is_empty() { None } else { v.parse().ok() }
}

pub fn get_i32(row: &Row, col: &str) -> Option<i32> {
    get_i64(row, col).map(|v| v as i32)
}

pub fn is_one(row: &Row, col: &str) -> bool {
    get(row, col).trim() == "1"
}

/// D2 treasureclass Item columns sometimes hold a quoted CSV pair like `"gld,mul=2048"`
/// (an item code plus an extra modifier annotation) — only the item code matters here.
pub fn first_csv_part(s: &str) -> String {
    s.split(',').next().unwrap_or("").trim().to_string()
}
