use anyhow::Result;
use drop_calc::{data, export, pipeline};
use clap::Parser;
use std::path::PathBuf;

/// D2R drop calculator: dumps every possible monster drop, across all difficulties, to a CSV you can
/// filter yourself in a spreadsheet.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Number of players in the game (1-8). Lowers the effective "nothing dropped" chance.
    #[arg(long, default_value_t = 1)]
    players: i32,

    /// Magic Find percentage.
    #[arg(long, default_value_t = 0)]
    magic_find: i64,

    /// Character level, used only to scale Terror Zone ("terrorized") rows.
    #[arg(long, default_value_t = 99)]
    character_level: i32,

    /// Output directory — writes base_items.csv, consumables_runes_gems.csv, and uniques_sets.csv here.
    #[arg(long, default_value = "drops")]
    output_dir: PathBuf,

    /// Drop rows rarer than "1 in X" (i.e. chance_per_x greater than this). Omit to keep everything.
    #[arg(long)]
    max_chance_per_x: Option<f64>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if !(1..=8).contains(&args.players) {
        anyhow::bail!("--players must be between 1 and 8");
    }
    if args.magic_find < 0 {
        anyhow::bail!("--magic-find cannot be negative");
    }
    if args.max_chance_per_x.is_some_and(|x| x <= 0.0) {
        anyhow::bail!("--max-chance-per-x must be positive");
    }

    eprintln!("Loading game data...");
    let game_data = data::GameData::load();

    let config = pipeline::Config {
        players: args.players,
        magic_find: args.magic_find,
        character_level: args.character_level,
    };

    eprintln!("Computing drop tables (players={}, magic_find={}, character_level={})...", config.players, config.magic_find, config.character_level);
    let rows = pipeline::generate(&game_data, &config);

    eprintln!("Writing {} rows to {}/...", rows.len(), args.output_dir.display());
    let summary = export::write_csvs_by_category(&args.output_dir, &rows, args.max_chance_per_x)?;
    for (file_name, count) in &summary {
        eprintln!("  {file_name}: {count} rows");
    }

    eprintln!("Done.");
    Ok(())
}
