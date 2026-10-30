# tools

Regenerates `data/area_monsters.tsv` (how many monsters of each kind an area spawns, which the
per-area view sums drops over) from a local Diablo II: Resurrected install, e.g. after a game patch.
Needs Python 3 (standard library only).

```sh
# 1. Pull the excel tables out of the game's archives (scans every data/*.data file, a few minutes
#    for ~40 GB).
py -I tools/extract_d2r.py "D:/SteamLibrary/steamapps/common/Diablo II Resurrected/data" <tables dir>

# 2. Write data/area_monsters.tsv from them (run from the repo root), and check it against a
#    simulation of the same spawn rules: every area, 2000 games each, ~30 s on all cores; exits
#    non-zero on a mismatch, and --out writes each row's error and z-score.
py -I tools/gen_area_monsters.py <tables dir>
py -I tools/sim_area_monsters.py <tables dir> --all [--runs N] [--seed N] [--out report.tsv]

# 3. Check the result.
cargo test
```

Keep the tables dir outside the repo: it holds the game's own files.

The spawn rules and their assumptions are described at the top of `gen_area_monsters.py`. Its
superunique and boss locations are listed by hand there, separately from the calculator's own in
`src/data/super_uniques.rs`; the two disagree on Ancient Kaa (any Tal Rasha tomb vs. one).
