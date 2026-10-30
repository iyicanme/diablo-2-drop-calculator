"""Checks data/area_monsters.tsv by rolling the same spawn rules many times and averaging.

usage: py -I tools/sim_area_monsters.py <dir from extract_d2r.py> [level ids... | --all] [--runs N]
                                        [--tsv data/area_monsters.tsv] [--out report.tsv]

gen_area_monsters.py works out exact averages; this plays the games out instead (written separately,
sharing only the tables, room layouts and boss locations), so a slip in the generator's maths shows
up as a mismatch. Both follow the same rules, so this finds bugs, not how far the rules are from the
real game.

For every monster/kind of every area and difficulty it compares the simulated average with the file:
error = simulated - file, z = error / standard error. With no bug, z is noise: about 5% of rows have
|z| > 2 and 0.3% |z| > 3, and the mean z is near 0. A row is flagged when |z| > 4 (and the error is
over 0.0001, the file's rounding). The summary also says how big an error could slip through
(4 standard errors, relative to the row's count): normal-monster rows are checked to a few percent,
rare ones (champions, crow nests) much more loosely, so raise --runs to check those closely.
--out writes every row with its error and z, for digging into a mismatch.

Within a room the pack ramp is fixed, so the number of attempts that spawn something is drawn in one
binomial roll and only those are played out; that gives the same distribution as rolling every
attempt. Areas run in parallel on all cores. Default areas: Blood Moor, Cold Plains, Den of Evil,
Catacombs Level 4, Tal Rasha's Tomb, Frigid Highlands.
"""
import argparse, csv, multiprocessing, os, random, sys

gen = None


def load(tables):
    """Imports gen_area_monsters, which reads its tables from sys.argv when imported."""
    global gen
    sys.argv = [sys.argv[0], tables]
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # py -I leaves the script's dir out
    import gen_area_monsters
    gen = gen_area_monsters


def flag(id, col):
    return gen.num(gen.MONSTATS[id][col]) == 1


def region_list(rng, level, suffix):
    """The area's monster list for one game: min(NumMon, 13) draws without replacement, the first
    redrawn up to 20 times until ranged when rangedspawn is set, keeping isSpawn monsters."""
    num = gen.num
    names = [level[f'{"mon" if not suffix else "nmon"}{i}'] for i in range(1, 26)]
    names = [n for n in names if n and n in gen.MONSTATS]
    k = min(num(level['NumMon']), 13, len(names))
    kept = []
    for i in range(k):
        pick = rng.randrange(len(names))
        if not i and num(level['rangedspawn']):
            for _ in range(20):
                if flag(names[pick], 'rangedtype'):
                    break
                pick = rng.randrange(len(names))
        id = names.pop(pick)
        if flag(id, 'isSpawn'):
            kept.append(id)
    return kept


def rarity_pick(rng, ids):
    num = gen.num
    roll = rng.randrange(sum(num(gen.MONSTATS[x]['Rarity']) for x in ids)) + 1
    for x in ids:
        roll -= num(gen.MONSTATS[x]['Rarity'])
        if roll <= 0:
            return x


def swap(rng, id, chance):
    m = gen.MONSTATS[id]
    if m['spawn'] and flag(id, 'placespawn') and rng.randrange(100) > chance:
        return m['spawn']
    return id


def one_game(rng, level, suffix, add):
    num, M = gen.num, gen.MONSTATS
    density = num(level['MonDen' + suffix])
    layouts = gen.room_sizes(level, suffix)
    if density and layouts:
        rooms = rng.choices([r for _, r in layouts], [p for p, _ in layouts])[0]
        region = region_list(rng, level, suffix)
        umon = [x for x in (level[f'umon{i}'] for i in range(1, 26)) if x and x in M]
        bmin, bmax = num(level['MonUMin' + suffix]), num(level['MonUMax' + suffix])
        hit = (min(density, 10000) + 1) / 100000
        count = 0
        for visited, (w, h) in enumerate(rooms, 1):
            if not region:
                break
            for _ in range(rng.binomialvariate(((w * 5 // 3) * h * 5) // 3, hit)):
                pack = (count < bmin and rng.randrange(100) < 100 * visited // len(rooms)) or \
                       (count < bmax and rng.randrange(100) <= 5)
                if pack:
                    if suffix:
                        leader = swap(rng, rarity_pick(rng, region), 0)
                    elif umon:
                        leader = rng.choice(umon)
                    else:
                        continue  # no candidate: nothing spawns
                    count += 1
                    if rng.randrange(100) < gen.CHAMPION_PCT:
                        extra = rng.randint(1, 3)
                        add(leader, 'champion', 1 + extra)
                        count += extra
                    else:
                        add(leader, 'unique', 1)
                        add(M[leader]['minion1'] or leader, 'minion', rng.randint(3, 6))
                else:
                    id = swap(rng, rarity_pick(rng, region), 20)
                    m = M[id]
                    sparse = num(m['sparsePopulate'])
                    if sparse and rng.randrange(100) > sparse:
                        continue
                    lo, hi = num(m['MinGrp']), num(m['MaxGrp'])
                    if (m['BaseId'] or id) in ('fallen1', 'scarab1') or not lo or hi < lo:
                        add(id, 'normal', 1)
                    else:
                        add(id, 'normal', 1 + (lo - 1) + rng.randrange(hi - lo + 1))
    id = num(level['Id'])
    for s in gen.SUPERUNIQUES:
        areas = gen.SUPERUNIQUE_AREAS.get(s['Superunique'], [])
        if id in areas and rng.choice(areas) == id:
            add(s['Superunique'], 'superunique', 1)
            if num(s['MaxGrp']):
                add(M[s['Class']]['minion1'] or s['Class'], 'minion', rng.randint(num(s['MinGrp']), num(s['MaxGrp'])))
    for boss, area in gen.BOSS_AREAS.items():
        if area == id:
            add(boss, 'boss', 1)


def simulate(task):
    """(level id, difficulty, runs, seed) -> {(monster, kind): (mean, standard error)}"""
    id, difficulty, runs, seed = task
    level = next(l for l in gen.LEVELS if gen.num(l['Id']) == id)
    suffix = dict(gen.DIFFICULTIES)[difficulty]
    rng = random.Random(f'{seed} {id} {difficulty}')
    sums, squares = {}, {}
    for _ in range(runs):
        game = {}

        def add(monster, kind, n):
            game[(monster, kind)] = game.get((monster, kind), 0) + n

        one_game(rng, level, suffix, add)
        for key, n in game.items():
            sums[key] = sums.get(key, 0) + n
            squares[key] = squares.get(key, 0) + n * n
    out = {}
    for key, s in sums.items():
        mean = s / runs
        out[key] = (mean, (max(0.0, squares[key] / runs - mean * mean) / runs) ** 0.5)
    return id, difficulty, out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('tables')
    parser.add_argument('levels', nargs='*', type=int)
    parser.add_argument('--all', action='store_true', help='every area')
    parser.add_argument('--runs', type=int, default=2000)
    parser.add_argument('--seed', type=int, default=0, help='rerun with fresh rolls to tell noise from bugs')
    parser.add_argument('--tsv', default='data/area_monsters.tsv')
    parser.add_argument('--out', help='write every row with its error to this file')
    args = parser.parse_args()
    load(args.tables)

    expected = {}
    with open(args.tsv, encoding='utf-8', newline='') as f:
        for r in csv.DictReader(f, delimiter='\t'):
            expected[(int(r['level_id']), r['difficulty'], r['monster'], r['kind'])] = float(r['count'])
    names = {gen.num(l['Id']): l['*StringName'] for l in gen.LEVELS}
    ids = sorted(names) if args.all else args.levels or [2, 3, 8, 37, 66, 111]
    tasks = [(id, d, args.runs, args.seed) for id in ids for d, _ in gen.DIFFICULTIES]
    with multiprocessing.Pool(initializer=load, initargs=(args.tables,)) as pool:
        results = pool.map(simulate, tasks, chunksize=1)

    rows = []  # (id, area, difficulty, monster, kind, file, simulated, se, error, z)
    for id, difficulty, sim in results:
        keys = set(sim) | {k[2:] for k in expected if k[:2] == (id, difficulty)}
        for key in sorted(keys):
            mean, se = sim.get(key, (0.0, 0.0))
            want = expected.get((id, difficulty) + key, 0.0)
            error = mean - want
            z = error / se if se else (0.0 if abs(error) < 0.0001 else float('inf'))
            rows.append((id, names[id], difficulty) + key + (want, mean, se, error, z))
    if args.out:
        with open(args.out, 'w', encoding='utf-8', newline='') as f:
            f.write('level_id\tarea\tdifficulty\tmonster\tkind\tfile\tsimulated\tse\terror\trel_error\tz\n')
            for r in rows:
                rel = f'{r[8] / r[5]:.4f}' if r[5] else ''
                f.write('\t'.join(map(str, r[:5])) + f'\t{r[5]:.4f}\t{r[6]:.4f}\t{r[7]:.4f}\t{r[8]:.4f}\t{rel}\t{r[9]:.2f}\n')

    off = [r for r in rows if abs(r[9]) > 4 and abs(r[8]) > 0.0001]
    finite = [r[9] for r in rows if r[7]]
    limits = sorted(4 * r[7] / r[5] for r in rows if r[5])
    print(f'{len(rows)} rows in {len(ids)} areas x 3 difficulties, {args.runs} games each')
    print(f'z: mean {sum(finite) / len(finite):+.3f}, |z|>2 {sum(abs(z) > 2 for z in finite) / len(finite):.1%} '
          f'(noise ~4.6%), |z|>3 {sum(abs(z) > 3 for z in finite) / len(finite):.2%} (noise ~0.27%)')
    print(f'errors that could slip through: under {limits[len(limits) // 2]:.1%} on half the rows, '
          f'under {limits[len(limits) * 9 // 10]:.0%} on 90%')
    print(f'{len(off)} rows off (|z| > 4):')
    for r in sorted(off, key=lambda r: -abs(r[9])):
        print(f'  {r[1]} ({r[0]}) {r[2]} {r[3]} {r[4]}: file {r[5]:.4f}, simulated {r[6]:.4f} +- {r[7]:.4f}')
    sys.exit(1 if off else 0)


if __name__ == '__main__':
    main()
