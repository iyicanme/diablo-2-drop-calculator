# Data provenance

The `.txt` and `.json` files in this directory are Diablo II: Resurrected's own game data (excel
tables and English string tables), extracted unchanged from a local D2R install by
`tools/extract_d2r.py`. The `.tsv` files are generated from those same game files:

- `chronicle_items.tsv` (the items the Chronicle tracks) by `tools/gen_chronicle_items.py`
- `item_details.tsv` (chronicle_tracker's Details pane) by `tools/gen_item_details.py`
- `area_monsters.tsv` (expected monsters of each kind per area clear) by `tools/gen_area_monsters.py`

See `tools/README.md` for how to refresh them after a game patch.

The data itself is Blizzard's and is used here strictly for a personal, non-commercial project.
The Rust code in `src/` that reads and interprets it is an independent implementation, designed
from D2 community mechanics documentation.
