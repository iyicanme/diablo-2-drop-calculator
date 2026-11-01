# tools

Refreshes everything in `data/` from a local Diablo II: Resurrected install, e.g. after a game
patch: the game's own tables and strings that the drop calculator and chronicle_tracker embed, and
the files generated from them (`chronicle_items.tsv`, the items the Chronicle tracks;
`item_details.tsv`, the stats, required levels, runeword bases and set bonuses shown in
chronicle_tracker's Details pane; `area_monsters.tsv`, how many monsters of each kind an area
spawns, for the web app's per-area view). Needs Python 3 (standard library only).

```sh
# 1. Pull the excel tables and English strings out of the game's archives (scans every
#    data/*.data file, a few minutes for ~40 GB).
py -I tools/extract_d2r.py "D:/SteamLibrary/steamapps/common/Diablo II Resurrected/data" <tables dir>

# 2. Copy the game files the Rust code embeds into data/ (run from the repo root).
for f in data/*.txt data/*.json; do cp "<tables dir>/$(basename "$f")" data/; done

# 3. Write data/chronicle_items.tsv, then data/item_details.tsv, from them.
py -I tools/gen_chronicle_items.py <tables dir>
py -I tools/gen_item_details.py <tables dir>

# 4. Write data/area_monsters.tsv from them, and check it against a simulation of the same spawn
#    rules: every area, 2000 games each, ~30 s on all cores; exits non-zero on a mismatch, and
#    --out writes each row's error and z-score.
py -I tools/gen_area_monsters.py <tables dir>
py -I tools/sim_area_monsters.py <tables dir> --all [--runs N] [--seed N] [--out report.tsv]

# 5. Check the result.
cargo test
```

Keep the tables dir outside the repo: it holds more of the game's files than the repo needs.

`gen_chronicle_items.py` lists every set item, unique and complete runeword whose row doesn't have
`disableChronicle` set, minus quest items. Its rows are the `*ID` column of `uniqueitems.txt` /
`setitems.txt`, and for runewords the data line index of `runes.txt` plus 27. The game calls all
eight Rainbow Facets "Rainbow Facet"; the list names them by element and trigger instead, so
`gen_item_details.py`, which prints any name that differs from the game's, reports those eight.
`gen_item_details.py` fails if an item gets no details.

`gen_area_monsters.py` lists the superunique and boss locations by hand, separately from the drop
calculator's own in `src/data/super_uniques.rs`; the two disagree on Ancient Kaa (any Tal Rasha
tomb vs. one).
