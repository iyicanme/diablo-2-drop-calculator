# tools

Regenerates `data/item_details.tsv` (the stats, required levels, runeword bases and set bonuses
shown in chronicle_tracker's Details pane) and `data/area_monsters.tsv` (how many monsters of each
kind an area spawns, for the web app's per-area view) from a local Diablo II: Resurrected install, e.g. after a game patch.
Needs Python 3 (standard library only).

```sh
# 1. Pull the excel tables and English strings out of the game's archives (scans every
#    data/*.data file, a few minutes for ~40 GB).
py -I tools/extract_d2r.py "D:/SteamLibrary/steamapps/common/Diablo II Resurrected/data" <tables dir>

# 2. Write data/item_details.tsv from them (run from the repo root).
py -I tools/gen_item_details.py <tables dir>

# 3. Write data/area_monsters.tsv from them, and check it against a simulation of the same spawn
#    rules: every area, 2000 games each, ~30 s on all cores; exits non-zero on a mismatch, and
#    --out writes each row's error and z-score.
py -I tools/gen_area_monsters.py <tables dir>
py -I tools/sim_area_monsters.py <tables dir> --all [--runs N] [--seed N] [--out report.tsv]

# 4. Check the result.
cargo test
```

Keep the tables dir outside the repo: it holds the game's own files.

`gen_item_details.py` prints any item or set name in `data/chronicle_items.tsv` whose spelling
differs from the game's (the item list uses the game's spelling, apart from the Rainbow Facets, which
the game all calls "Rainbow Facet"), and fails if an item gets no details. New items in a patch need
adding to `data/chronicle_items.tsv` first; their rows are the `*ID` column of `uniqueitems.txt` /
`setitems.txt`, and for runewords the data line index of `runes.txt` plus 27.

`gen_area_monsters.py` lists the superunique and boss locations by hand, separately from the drop
calculator's own in `src/data/super_uniques.rs`; the two disagree on Ancient Kaa (any Tal Rasha
tomb vs. one).
