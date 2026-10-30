"""Builds data/area_monsters.tsv: how many monsters of each kind clearing an area spawns, on average.

usage: py -I tools/gen_area_monsters.py <dir from extract_d2r.py> [data/area_monsters.tsv]

The counts are exact expected values of the game's random spawn rolls, worked out without sampling.
The rolls follow the reverse-engineered game code (D2MOO: D2Game MONSTER/MonsterRegion.cpp,
MonsterChoose.cpp, MonsterUnique.cpp):
- Every room gets ((W/3)*H)/3 spawn attempts (W, H in subtiles); each spawns something when
  rand%100000 <= MonDen.
- An attempt makes a champion/unique pack when the area's pack count is below MonUMin and
  rand%100 < 100*roomsVisited/rooms, or below MonUMax and rand%100 <= 5; otherwise a normal group.
  A pack leader is a champion 20% of the time (monumod.txt) with 1-3 champion minions, else a unique
  with 3-6 minions of its monstats minion1. Champions and their minions all add to the pack count.
- A normal group is a rarity weighted pick from the area's monster list of MinGrp-MaxGrp monsters
  (fallen and scarabs always alone). The list is min(NumMon, 13) monsters drawn from levels.txt
  mon1.. (nmon1.. in Nightmare/Hell); Normal pack leaders come from umon1.. instead.
Not dice, so assumed: every room of the area is populated, outdoor and preset areas are cut into
8x8 tile rooms, maze areas have the lvlmaze.txt room count (3x and 2x in two random Tal Rasha tombs),
and rooms are visited in a fixed order.
On top come the superuniques and act bosses, whose areas are listed below by hand (their spawn spots
are in the map files, not the tables). Not counted: other preset monsters, critters, wanderers.
"""
import csv, itertools, os, sys

X = sys.argv[1]
OUT = sys.argv[2] if len(sys.argv) > 2 else 'data/area_monsters.tsv'
DIFFICULTIES = [('normal', ''), ('nightmare', '(N)'), ('hell', '(H)')]
ROOM = 8  # tiles per side of an outdoor/preset room
SUBTILES = 5  # subtiles per tile

# superuniques.txt key -> level Ids it may spawn in (equally likely), from the Arreat Summit
SUPERUNIQUE_AREAS = {
    'Bishibosh': [3], 'Bonebreak': [18], 'Coldcrow': [9], 'Rakanishu': [4], 'Treehead WoodFist': [5],
    'Griswold': [38], 'The Countess': [25], 'Pitspawn Fouldog': [30], 'Boneash': [33],
    'Corpsefire': [8], 'The Smith': [28], 'The Cow King': [39],
    'Radament': [49], 'Bloodwitch the Wild': [60], 'Fangskin': [61], 'Beetleburst': [43],
    'Leatherarm': [59], 'Coldworm the Burrower': [64], 'Fire Eye': [54], 'Dark Elder': [44],
    'The Summoner': [74], 'Ancient Kaa the Soulless': [66, 67, 68, 69, 70, 71, 72],
    'Web Mage the Burning': [85], 'Witch Doctor Endugu': [91], 'Stormtree': [79],
    'Sarina the Battlemaid': [94], 'Icehawk Riftwing': [92], 'Ismail Vilehand': [83],
    'Geleb Flamefinger': [83], 'Toorc Icefist': [83], 'Bremm Sparkfist': [102],
    'Wyand Voidfinger': [102], 'Maffer Dragonhand': [102],
    'Infector of Souls': [108], 'Lord De Seis': [108], 'Grand Vizier of Chaos': [108],
    'The Feature Creep': [107],
    'Siege Boss': [110], 'Dac Farren': [110], 'Megaflow Rectifier': [111], 'Eyeback Unleashed': [111],
    'Sharp Tooth Sayer': [111], 'Threash Socket': [112], 'Frozenstein': [114], 'Bonesaw Breaker': [115],
    'Snapchip Shatter': [119], 'Ancient Barbarian 1': [120], 'Ancient Barbarian 2': [120],
    'Ancient Barbarian 3': [120], 'Pindleskin': [121], 'Nihlathak Boss': [124],
    'Baal Subject 1': [131], 'Baal Subject 2': [131], 'Baal Subject 3': [131], 'Baal Subject 4': [131],
    'Baal Subject 5': [131],
}
# never placed in the game
UNUSED_SUPERUNIQUES = {'Flamespike the Crawler', 'Winged Death', 'The Tormentor', 'Taintbreeder',
                       'Riftwraith the Cannibal', 'Axe Dweller', 'Anodized Elite', 'Vinvear Molech',
                       'Magma Torquer', 'Blaze Ripper'}
# monstats Id -> level Id, for the act bosses and other placed uniques that aren't superuniques
BOSS_AREAS = {'bloodraven': 17, 'andariel': 37, 'duriel': 73, 'mephisto': 102, 'izual': 105,
              'diablo': 108, 'baalcrab': 132}
TAL_RASHA_TOMBS = range(66, 73)


def table(name):
    with open(os.path.join(X, name + '.txt'), encoding='latin1', newline='') as f:
        rows = list(csv.DictReader(f, delimiter='\t'))
    return [{k: v.strip() if isinstance(v, str) else v for k, v in r.items()} for r in rows]


def num(v):
    return int(v) if v not in ('', None) else 0


MONSTATS = {r['Id']: r for r in table('monstats')}
LEVELS = [r for r in table('levels') if r['Id'].isdigit()]
MAZES = {}
for r in table('lvlmaze'):
    if num(r['Level']):
        MAZES.setdefault(num(r['Level']), []).append(r)
PRESET_POPULATE = {num(r['LevelId']): num(r['Populate']) for r in table('lvlprest') if num(r['LevelId'])}
CHAMPION_PCT = num(table('monumod')[0]['constants'])
SUPERUNIQUES = [r for r in table('superuniques') if r['Superunique'] and r['Class']]


def mon(id):
    return MONSTATS[id]


def flag(id, col):
    return num(mon(id)[col]) == 1


def minion_of(id):
    return mon(id)['minion1'] or id


def placed(id, chance):
    """{monster: probability} after the placespawn swap: when rand%100 > chance, a monster with a spawn
    column (e.g. a crow nest) is replaced by what it spawns."""
    m = mon(id)
    if m['spawn'] and flag(id, 'placespawn'):
        swap = (99 - chance) / 100
        return {id: 1 - swap, m['spawn']: swap}
    return {id: 1.0}


def group_size(id):
    """Expected size of a normal group: a leader plus rand(MinGrp-1 .. MaxGrp-1) more."""
    m = mon(id)
    if (m['BaseId'] or id) in ('fallen1', 'scarab1'):
        return 1.0
    lo, hi = num(m['MinGrp']), num(m['MaxGrp'])
    return 1 + max(0, (lo - 1 + hi - 1) / 2) if lo and hi >= lo else 1.0


def region_shares(level, suffix):
    """(P(the area's monster list keeps at least one monster), {monster: expected rarity share given
    that}): the list is min(NumMon, 13) entries drawn without replacement, keeping isSpawn ones."""
    names = [level[f'{"mon" if not suffix else "nmon"}{i}'] for i in range(1, 26)]
    names = [n for n in names if n and n in MONSTATS]
    k = min(num(level['NumMon']), 13, len(names))
    if not k:
        return 0.0, {}
    n = len(names)
    ranged = [i for i, x in enumerate(names) if flag(x, 'rangedtype')]
    if num(level['rangedspawn']) and ranged and len(ranged) < n:
        # the first draw is redrawn up to 20 times until it is a ranged monster
        miss = ((n - len(ranged)) / n) ** 21
        first = {i: ((1 - miss) / len(ranged) if i in ranged else miss / (n - len(ranged))) for i in range(n)}
    else:
        first = {i: 1 / n for i in range(n)}
    nonempty, shares = 0.0, {}
    for f, pf in first.items():
        rest = [i for i in range(n) if i != f]
        combos = list(itertools.combinations(rest, k - 1))
        for combo in combos:
            p = pf / len(combos)
            kept = [names[i] for i in (f,) + combo if flag(names[i], 'isSpawn')]
            total = sum(num(mon(x)['Rarity']) for x in kept)
            if not total:
                continue
            nonempty += p
            for x in kept:
                shares[x] = shares.get(x, 0) + p * num(mon(x)['Rarity']) / total
    return nonempty, {x: s / nonempty for x, s in shares.items()} if nonempty else {}


def outdoor_size(level, suffix):
    """SizeX, SizeY in tiles. A few Act 5 outdoor areas have -1 there (sized somewhere outside the
    tables), so they borrow the size of the closest area with the same LevelType."""
    w, h = num(level['SizeX' + suffix]), num(level['SizeY' + suffix])
    if w > 0 and h > 0:
        return w, h
    id = num(level['Id'])
    like = [l for l in LEVELS if l['LevelType'] == level['LevelType'] and l['DrlgType'] == level['DrlgType']
            and num(l['SizeX' + suffix]) > 0 and num(l['SizeY' + suffix]) > 0]
    if not like:
        return 0, 0
    other = min(like, key=lambda l: abs(num(l['Id']) - id))
    return num(other['SizeX' + suffix]), num(other['SizeY' + suffix])


def room_sizes(level, suffix):
    """[(probability, [room sizes in tiles])] for the layouts the area can have."""
    drlg, id = num(level['DrlgType']), num(level['Id'])
    if drlg == 1:
        mazes = MAZES.get(id, [])
        # one of the seven Tal Rasha tombs gets 3x the rooms (the staff tomb), another 2x (Duriel's)
        scales = [(1 / 7, 3), (1 / 7, 2), (5 / 7, 1)] if id in TAL_RASHA_TOMBS else [(1, 1)]
        return [(p / len(mazes), [(num(r['SizeX']), num(r['SizeY']))] * num(r['Rooms' + suffix]) * scale)
                for r in mazes for p, scale in scales]
    if drlg == 2 and not PRESET_POPULATE.get(id, 1):
        return []
    w, h = outdoor_size(level, suffix)
    return [(1, [(min(ROOM, w - x), min(ROOM, h - y)) for y in range(0, h, ROOM) for x in range(0, w, ROOM)])]


def expected_groups(rooms, density, boss_min, boss_max, champion_extra):
    """Exact expected (normal groups, packs, unique leaders, champion packs) over all rolls, tracking
    the distribution of the area's pack count as rooms are populated."""
    top = max(boss_min, boss_max)
    dist = [1.0] + [0.0] * top  # dist[c] = P(pack count == c), the last bucket meaning >= top
    hit = (min(density, 10000) + 1) / 100000
    groups = packs = 0.0
    for visited, (w, h) in enumerate(rooms, 1):
        ramp = min(100, 100 * visited // len(rooms)) / 100
        attempts = ((w * SUBTILES // 3) * h * SUBTILES) // 3
        pack_chance = []
        for c in range(top + 1):
            forced = ramp if c < boss_min else 0
            pack_chance.append(forced + (1 - forced) * (0.06 if c < boss_max else 0))
        for _ in range(attempts):
            new = [0.0] * (top + 1)
            for c, p in enumerate(dist):
                if not p:
                    continue
                pp = p * hit * pack_chance[c]
                groups += p * hit - pp
                packs += pp
                new[c] += p - pp
                if pp:
                    new[min(c + 1, top)] += pp * (1 - CHAMPION_PCT / 100)
                    for extra, q in champion_extra.items():
                        new[min(c + 1 + extra, top)] += pp * CHAMPION_PCT / 100 * q
            dist = new
    return groups, packs


def area_counts(level, suffix):
    """{(monster, kind): expected count} from the random spawns of one area in one difficulty."""
    density = num(level['MonDen' + suffix])
    layouts = room_sizes(level, suffix)
    if not density or not layouts:
        return {}
    nonempty, shares = region_shares(level, suffix)
    if not nonempty:
        return {}
    if suffix:  # Nightmare/Hell pack leaders are picked like normal groups
        leaders = shares
    else:
        umon = [level[f'umon{i}'] for i in range(1, 26)]
        umon = [x for x in umon if x and x in MONSTATS]
        leaders = {x: umon.count(x) / len(umon) for x in set(umon)}
    boss_min, boss_max = num(level['MonUMin' + suffix]), num(level['MonUMax' + suffix])
    counts = {}

    def add(id, kind, n):
        counts[(id, kind)] = counts.get((id, kind), 0) + n

    for chance, rooms in layouts:
        if not rooms:
            continue
        weight = nonempty * chance
        # a pack leader with no candidate spawns nothing and doesn't count
        groups, packs = expected_groups(rooms, density, boss_min if leaders else 0, boss_max if leaders else 0,
                                        {1: 1 / 3, 2: 1 / 3, 3: 1 / 3})
        for x, s in shares.items():
            for y, q in placed(x, 20).items():
                sparse = num(mon(y)['sparsePopulate'])
                spawn = (sparse + 1) / 100 if sparse else 1
                add(y, 'normal', weight * groups * s * q * spawn * group_size(y))
        for x, s in leaders.items():
            for y, q in (placed(x, 0) if suffix else {x: 1.0}).items():
                n = weight * packs * s * q
                add(y, 'champion', n * CHAMPION_PCT / 100 * 3)  # leader + 1-3 champion minions
                add(y, 'unique', n * (1 - CHAMPION_PCT / 100))
                add(minion_of(y), 'minion', n * (1 - CHAMPION_PCT / 100) * 4.5)  # 3-6 minions
    return counts


def main():
    names = {num(l['Id']): l['*StringName'] for l in LEVELS}
    unmapped = [s['Superunique'] for s in SUPERUNIQUES
                if s['Superunique'] not in SUPERUNIQUE_AREAS and s['Superunique'] not in UNUSED_SUPERUNIQUES]
    if unmapped:
        sys.exit(f'superuniques with no area: {unmapped}')
    rows = []
    for level in LEVELS:
        id = num(level['Id'])
        if level['DrlgType'] != '1' and num(level['MonDen']) and num(level['SizeX']) <= 0:
            print(f'{level["*StringName"]} has no size, using a {outdoor_size(level, "")} tile one')
        for difficulty, suffix in DIFFICULTIES:
            counts = area_counts(level, suffix)
            for s in SUPERUNIQUES:
                areas = SUPERUNIQUE_AREAS.get(s['Superunique'], [])
                if id in areas:
                    counts[(s['Superunique'], 'superunique')] = 1 / len(areas)
                    lo, hi = num(s['MinGrp']), num(s['MaxGrp'])
                    if hi:
                        key = (minion_of(s['Class']), 'minion')
                        counts[key] = counts.get(key, 0) + (lo + hi) / 2 / len(areas)
            for boss, area in BOSS_AREAS.items():
                if area == id:
                    counts[(boss, 'boss')] = 1
            for (monster, kind), n in counts.items():
                if n >= 0.00005:
                    rows.append((id, names[id], difficulty, monster, kind, n))
    rows.sort(key=lambda r: (r[0], [d for d, _ in DIFFICULTIES].index(r[2]), r[3], r[4]))
    with open(OUT, 'w', encoding='utf-8', newline='') as f:
        f.write('level_id\tarea\tdifficulty\tmonster\tkind\tcount\n')
        for id, area, difficulty, monster, kind, n in rows:
            f.write(f'{id}\t{area}\t{difficulty}\t{monster}\t{kind}\t{n:.4f}\n')
    print(f'{len(rows)} rows -> {OUT}')


if __name__ == '__main__':
    main()
