# Data provenance

The `.txt` and `.json` files in this directory are Diablo II: Resurrected's own game data
(monster stats, treasure classes, item tables, level population, and string tables), sourced via
the `silospen/DropCalc` GitHub repository's bundled `D2R_ROW_3.0` data snapshot.

This is used here strictly for a personal, non-commercial project and is not redistributed further.
The Rust code in `src/` that reads and interprets this data is an independent reimplementation —
it was designed by reading D2 community mechanics documentation and, for validating column meanings
and algorithm details, the DropCalc source (which carries no explicit license) for understanding
only, not by copying its code.

The display-only files used for the web app's hover cards — `monlvl.json`, `properties.json`,
`skills.json`, `skilldesc.json`, `localestrings-eng.json` and `gems.json` — are the same D2R 3.0 game data,
taken in JSON form from the `blizzhackers/d2data` GitHub repository (MIT-licensed tooling; the
data itself remains Blizzard's). Same personal, non-commercial terms as above.
