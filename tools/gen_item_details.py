"""Builds data/item_details.tsv from the game's excel tables and English strings.

usage: py -I tools/gen_item_details.py <dir from extract_d2r.py> [data/chronicle_items.tsv] [data/item_details.tsv]

Stat lines are formatted the way the game's tooltips are: properties.txt (and propertygroups.txt)
expand each item property into stats, itemstatcost.txt says how each stat is described and in what
order, and the strings fill in the text. Rolled stats show their range, e.g. "+[10-15] to Strength".
"""
import csv, json, os, re, sys

X = sys.argv[1]
ITEMS_TSV = sys.argv[2] if len(sys.argv) > 2 else 'data/chronicle_items.tsv'
OUT = sys.argv[3] if len(sys.argv) > 3 else 'data/item_details.tsv'
RUNEWORD_ROW_OFFSET = 27  # Chronicle runeword row = runes.txt data line index + 27


def table(name):
    with open(os.path.join(X, name + '.txt'), encoding='latin1', newline='') as f:
        rows = list(csv.DictReader(f, delimiter='\t'))
    return [{k: v.strip() if isinstance(v, str) else v for k, v in r.items()} for r in rows]


STR = {}
for name in ['item-modifiers', 'item-names', 'item-runes', 'skills', 'item-nameaffixes']:
    with open(os.path.join(X, name + '.json'), encoding='utf-8-sig') as f:
        for e in json.load(f):
            STR.setdefault(e['Key'], e['enUS'])


def s(key):
    return STR.get(key, key)


ISC = {r['Stat']: r for r in table('itemstatcost') if r['Stat']}
PROPS = {r['code'].lower(): r for r in table('properties') if r['code']}
PGROUPS = {r['code'].lower(): r for r in table('propertygroups') if r['code']}
SKILLS = [r for r in table('skills') if r['skill']]
SKILL_BY_ID = {int(r['*Id']): r for r in SKILLS if r['*Id']}
SKILL_BY_NAME = {r['skill'].lower(): r for r in SKILLS}
SKILLDESC = {r['skilldesc']: r for r in table('skilldesc') if r['skilldesc']}
CLASSES = [r for r in table('charstats') if r['class'] and r['class'] != 'Expansion']
ITYPES = {r['Code']: r for r in table('itemtypes') if r['Code']}
MONSTERS = [r for r in table('monstats') if r['Id']]
GEMS = {g['code']: g for g in table('gems') if g['code']}
BASES = {r['code']: r for t in ['weapons', 'armor', 'misc'] for r in table(t) if r.get('code')}


def num(v):
    try:
        return int(v)
    except (TypeError, ValueError):
        return 0


def skill_row(par):
    par = str(par)
    return SKILL_BY_ID.get(int(par)) if par.lstrip('-').isdigit() else SKILL_BY_NAME.get(par.lower())


def skill(par):
    r = skill_row(par)
    if r is None:
        return str(par)
    d = SKILLDESC.get(r['skilldesc'])
    return s(d['str name']) if d and d.get('str name') else r['skill']


def skill_class(par):
    """'(Necromancer Only)' for a class skill, else ''."""
    r = skill_row(par)
    code = r['charclass'] if r else ''
    return next((s(c['StrClassOnly']) for c in CLASSES if code and c['class'][:3].lower() == code[:3]), '')


class V:
    """A rolled value: min..max."""

    def __init__(self, lo, hi=None):
        hi = lo if hi is None else hi
        self.lo, self.hi = min(lo, hi), max(lo, hi)

    def same(self):
        return self.lo == self.hi

    def fmt(self, signed=False):
        if self.same():
            return f'{self.lo:+d}' if signed else str(self.lo)
        sign = '+' if signed and self.lo >= 0 else ''
        return f'{sign}[{self.lo}-{self.hi}]'

    def __eq__(self, o):
        return (self.lo, self.hi) == (o.lo, o.hi)


def printf(fmt, *args, numbered=()):
    """Fills %d/%+d/%i/%s in order with args, and %0/%1 with numbered[0]/[1]."""
    args = list(args)
    out, pos = [], 0
    for m in re.finditer(r'%(%|\+?d|i|s|\d)', fmt):
        out.append(fmt[pos:m.start()])
        tok = m.group(1)
        if tok == '%':
            out.append('%')
        elif tok.isdigit():
            n = int(tok)
            out.append(str(numbered[n]) if n < len(numbered) else '')
        else:
            a = args.pop(0) if args else ''
            out.append(a.fmt(signed=tok == '+d') if isinstance(a, V) else str(a))
        pos = m.end()
    out.append(fmt[pos:])
    return ''.join(out)


def desc(stat, v, par=None):
    """One stat line, or None if the stat has no description."""
    r = ISC.get(stat)
    if r is None or not r['descfunc']:
        return None
    func = int(r['descfunc'])
    fmt = s(r['descstrneg'] if v.hi < 0 and r['descstrneg'] else r['descstrpos'])
    extra = ' ' + s(r['descstr2']) if r['descstr2'] else ''
    if stat.endswith('_perlevel') or func in (6, 7, 8, 9, 10):
        # Per-level stats store 8 times the value gained per character level.
        lo, hi = (par or v.lo) / 8, (par or v.hi) / 8
        val = f'{lo:g}' if lo == hi else f'[{lo:g}-{hi:g}]'
        line = printf(fmt, f'+{val}' if '%+d' in fmt else val) if '%' in fmt else f'+{val} {fmt}'
        return line + ' per Character Level'
    if func == 13:  # +x to class skills
        return printf(s(CLASSES[num(par)]['StrAllSkills']), v)
    if func == 14:  # +x to a skill tab; param = class * 3 + tab
        c = CLASSES[num(par) // 3]
        return printf(s(c[f'StrSkillTab{num(par) % 3 + 1}']), v) + ' ' + s(c['StrClassOnly'])
    if func == 16:  # aura
        return printf(fmt, v, skill(par))
    if func in (27, 28):  # +x to a skill (27: class only)
        cls = skill_class(par) if func == 27 else ''
        return f'{printf(fmt, v, skill(par))} {cls}'
    if func == 23:  # reanimate as
        mon = MONSTERS[num(par)] if num(par) < len(MONSTERS) else None
        return printf(fmt, numbered=(v.fmt(), s(mon['NameStr']) if mon else str(par)))
    if func == 11:  # repairs 1 durability in n seconds
        if v.same() and v.lo:
            return printf(fmt, 1, max(1, round(100 / v.lo)))
        return printf(fmt, 1, f'[{round(100 / v.hi)}-{round(100 / v.lo)}]')
    if '%' in fmt:
        return printf(fmt, v) + extra
    # Strings without placeholders: no value (descval 0), value before (1) or after (2) the text.
    descval = num(r['descval'])
    if func == 3 or descval == 0:
        return fmt + extra
    val = v.fmt(signed=func in (1, 2, 4, 12)) + ('%' if func in (2, 5) else '')
    return (f'{val} {fmt}' if descval == 1 else f'{fmt} {val}') + extra


def expand(code, par, lo, hi):
    """Property -> [(stat, V, par)] following its properties.txt funcs. Stats starting with '#'
    are described specially, '?' marks an unknown property."""
    p = PROPS.get(code.lower())
    if p is None:
        return [('?' + code, V(lo, hi), par)]
    if code == 'dmg-pois':
        # par = duration in frames, min/max = damage per frame * 256
        n = num(par)
        return [('#poison', V(round(lo * n / 256), round(hi * n / 256)), n // 25)]
    out, prev = [], None
    for i in range(1, 8):
        func, stat = num(p[f'func{i}']), p[f'stat{i}']
        if not func:
            continue
        v = V(lo, hi)
        if func in (1, 2, 8):
            out.append((stat, v, par))
            prev = v
        elif func == 3:  # same value as the previous stat
            out.append((stat, prev or v, par))
        elif func == 5:
            out.append(('mindamage', v, par))
        elif func == 6:
            out.append(('maxdamage', v, par))
        elif func == 7:
            out += [('item_maxdamage_percent', v, par), ('item_mindamage_percent', v, par)]
        elif func == 11:  # chance to cast: min = chance %, max = skill level
            out.append((stat, V(lo), (par, hi)))
        elif func == 12:  # random skill: par = level, min..max = skill ids
            out.append(('#skill-rand', V(num(par)), (lo, hi)))
        elif func == 14:
            out.append(('item_numsockets', v, par))
        elif func == 15:
            out.append((stat, V(lo), par))
        elif func == 16:
            out.append((stat, V(hi), par))
        elif func == 17:  # per level: the value is the param
            out.append((stat, V(num(par)), num(par)))
        elif func == 20:
            out.append(('item_indesctructible', V(1), par))
        elif func == 21:  # class skills: the class is the property's val
            out.append((stat, v, num(p[f'val{i}'])))
        elif func == 23:
            out.append(('#ethereal', v, par))
        elif func == 36:
            out.append(('#randclassskill', v, par))
        else:
            out.append((stat or '?' + code, v, par))
    if code == 'dmg-cold':
        out = [x for x in out if x[0] != 'coldlength']
    return out


# Stats shown as one line when all are present with the same value.
GROUPS = [
    (('fireresist', 'lightresist', 'coldresist', 'poisonresist'), 'strModAllResistances'),
    (('strength', 'energy', 'dexterity', 'vitality'), 'Moditem2allattrib'),
]
# Min/max damage pairs shown as one line: (min stat, max stat, range string, equal string).
RANGES = [
    ('firemindam', 'firemaxdam', 'strModFireDamageRange', 'strModFireDamage'),
    ('lightmindam', 'lightmaxdam', 'strModLightningDamageRange', 'strModLightningDamage'),
    ('coldmindam', 'coldmaxdam', 'strModColdDamageRange', 'strModColdDamage'),
    ('magicmindam', 'magicmaxdam', 'strModMagicDamageRange', 'strModMagicDamage'),
    ('mindamage', 'maxdamage', 'strModMinDamageRange', 'strModMinDamage'),
]
ON_EVENT = {'item_skillonhit', 'item_skillonattack', 'item_skillongethit', 'item_skillonkill',
            'item_skillondeath', 'item_skillonlevelup', 'item_skillonstrike', 'item_skillondamaged'}


def prio(stat):
    r = ISC.get(stat)
    return num(r['descpriority']) if r else 0


def property_group(group):
    """(priority, line) for a property group, which rolls one of its options."""
    options, best = [], 0
    for i in range(1, 9):
        p = group.get(f'Prop{i}')
        if not p:
            continue
        pmin, pmax = group[f'ParMin{i}'], group[f'ParMax{i}']
        params = range(num(pmin), num(pmax) + 1) if pmin.isdigit() and pmax.isdigit() and pmin != pmax else [pmin]
        for par in params:
            options += lines([(p, par, num(group[f'ModMin{i}']), num(group[f'ModMax{i}']))])
        best = max([best] + [prio(st) for st, _, _ in expand(p, pmin, 0, 0)])
    return best, 'One of: ' + ' / '.join(options)


def lines(props):
    """props: [(code, par, min, max)] -> description lines, in tooltip order."""
    stats, out = [], []  # out: (priority, text)
    for code, par, lo, hi in props:
        if code.startswith('*'):  # commented out in the table
            continue
        group = PGROUPS.get(code.lower())
        if group is None:
            stats += expand(code, par, lo, hi)
        else:
            out.append(property_group(group))
    # The same stat from several sources (a runeword and its runes) adds up.
    merged = {}
    for st, v, par in stats:
        key = (st, str(par))
        if key in merged and not st.startswith('#') and not isinstance(par, tuple) and st != 'item_charged_skill':
            old = merged[key][1]
            merged[key] = (st, V(old.lo + v.lo, old.hi + v.hi), par)
        else:
            merged.setdefault(key, (st, v, par))
    stats = list(merged.values())
    have = {}
    for st, v, par in stats:
        have.setdefault(st, (v, par))
    used = set()
    for group, key in GROUPS:
        vals = [have.get(g) for g in group]
        if all(vals) and all(x[0] == vals[0][0] for x in vals):
            out.append((prio(group[0]), printf(s(key), vals[0][0])))
            used.update(group)
    for mn, mx, rng, same in RANGES:
        if mn in have and mx in have and mn not in used:
            a, b = have[mn][0], have[mx][0]
            if mn != 'mindamage' and a == b and s(same) != same:
                text = printf(s(same), a)
            else:
                text = printf(s(rng), a, b)
            out.append((prio(mx), text))
            used.update((mn, mx))
    for st, v, par in stats:
        if st in used:
            continue
        if st == 'item_mindamage_percent' and 'item_maxdamage_percent' in have:
            continue
        if st == 'item_maxdamage_percent' and have.get('item_mindamage_percent', (None,))[0] == v:
            out.append((prio(st), printf(s('strModEnhancedDamage'), v)))
        elif st == '#poison':
            out.append((prio('poisonmaxdam'), printf(s('strModPoisonDamage'), v, V(par))))
        elif st == '#ethereal':
            out.append((-1, s('strethereal')))
        elif st == '#skill-rand':
            names = [skill(i) for i in range(num(par[0]), num(par[1]) + 1)]
            out.append((prio('item_singleskill'), f'+{v.lo} to a random skill: ' + ' / '.join(names)))
        elif st == '#randclassskill':
            out.append((prio('item_addclassskills'), f'{v.fmt(True)} to a random Class Skill Levels'))
        elif st == 'item_numsockets':
            out.append((-2, printf(s('Socketable'), v)))
        elif st in ON_EVENT:
            sk, lvl = par
            out.append((prio(st), printf(s(ISC[st]['descstrpos']), V(v.lo), V(num(lvl)), skill(sk))))
        elif st == 'item_charged_skill':
            out.append((prio(st), printf(s(ISC[st]['descstrpos']), V(v.hi), skill(par), V(v.lo), V(v.lo))))
        elif st.startswith('?'):
            print('dropping unknown property', st[1:], file=sys.stderr)  # the game ignores these too
        elif text := desc(st, v, par):
            out.append((prio(st), text))
    out.sort(key=lambda t: -t[0])
    return [re.sub(r'\s+', ' ', t).strip() for _, t in out if t.strip()]


def props_of(r, n):
    """prop1..n of a uniqueitems/setitems row."""
    return [(r[f'prop{i}'], r.get(f'par{i}', ''), num(r.get(f'min{i}')), num(r.get(f'max{i}')))
            for i in range(1, n + 1) if r.get(f'prop{i}')]


ITEMS = {}  # (kind, row) -> name, as the item list spells it
TSV_SET = {}  # (kind, row) -> set name, likewise
with open(ITEMS_TSV, encoding='utf-8') as f:
    for l in f:
        if l.startswith('#') or not l.strip():
            continue
        cols = l.rstrip('\n').split('\t')
        for row in cols[1].split(','):
            ITEMS[(cols[0], int(row))] = cols[2]
            TSV_SET[(cols[0], int(row))] = cols[4]
SET_NAMES = {}  # sets.txt index -> set name as the item list spells it

out, renamed = [], []


def emit(kind, key, field, value):
    out.append(f'{kind}\t{key}\t{field}\t{value}')


def check(kind, row, display):
    if ITEMS[(kind, row)] != display:
        renamed.append(f'{kind} {row}: {ITEMS[(kind, row)]!r} is {display!r} in game')


for r in table('uniqueitems'):
    if not r['*ID'] or ('unique', int(r['*ID'])) not in ITEMS:
        continue
    row = int(r['*ID'])
    check('unique', row, s(r['index']))
    emit('unique', row, 'req', r['lvl req'])
    for t in lines(props_of(r, 12)):
        emit('unique', row, 'stat', t)

for r in table('setitems'):
    if not r['*ID'] or ('set', int(r['*ID'])) not in ITEMS:
        continue
    row = int(r['*ID'])
    check('set', row, s(r['index']))
    SET_NAMES[r['set']] = TSV_SET[('set', row)]
    if TSV_SET[('set', row)] != s(r['set']):
        renamed.append(f'set of {ITEMS[("set", row)]}: {TSV_SET[("set", row)]!r} is {s(r["set"])!r} in game')
    emit('set', row, 'req', r['lvl req'])
    for t in lines(props_of(r, 9)):
        emit('set', row, 'stat', t)
    # Bonuses for wearing more of the set: aprop{n}a/b apply with n + 1 pieces worn.
    for n in range(1, 6):
        bonus = [(r[f'aprop{n}{x}'], r.get(f'apar{n}{x}', ''), num(r.get(f'amin{n}{x}')), num(r.get(f'amax{n}{x}')))
                 for x in 'ab' if r.get(f'aprop{n}{x}')]
        for t in lines(bonus):
            emit('set', row, 'bonus', f'({n + 1} items) {t}')

for r in table('sets'):
    name = SET_NAMES.get(r['index'])
    if name is None:  # no tracked pieces
        continue
    for n in range(2, 6):
        bonus = [(r[f'PCode{n}{x}'], r[f'PParam{n}{x}'], num(r[f'PMin{n}{x}']), num(r[f'PMax{n}{x}']))
                 for x in 'ab' if r.get(f'PCode{n}{x}')]
        for t in lines(bonus):
            emit('setbonus', name, 'partial', f'({n} items) {t}')
    full = [(r[f'FCode{i}'], r[f'FParam{i}'], num(r[f'FMin{i}']), num(r[f'FMax{i}']))
            for i in range(1, 9) if r.get(f'FCode{i}')]
    for t in lines(full):
        emit('setbonus', name, 'full', t)

TYPE_NAMES = {
    'tors': 'Body Armor', 'shie': 'Shield', 'shld': 'Shield', 'weap': 'Weapon', 'mele': 'Melee Weapon',
    'miss': 'Missile Weapon', 'helm': 'Helm', 'pala': 'Paladin Shield', 'h2h': 'Claw', 'knif': 'Dagger',
    'swor': 'Sword', 'hamm': 'Hammer', 'scep': 'Scepter', 'staf': 'Staff', 'wand': 'Wand', 'rod': 'Staff/Rod',
    'blun': 'Blunt Weapon', 'spea': 'Spear', 'pole': 'Polearm', 'mace': 'Mace', 'club': 'Club', 'axe': 'Axe',
    'bow': 'Bow', 'xbow': 'Crossbow', 'jave': 'Javelin', 'thro': 'Throwing Weapon', 'circ': 'Circlet',
    'phlm': 'Barbarian Helm', 'pelt': 'Druid Pelt', 'head': 'Necromancer Shrunken Head', 'ashd': 'Paladin Shield',
    'orb': 'Sorceress Orb', 'abow': 'Amazon Bow', 'aspe': 'Amazon Spear', 'ajav': 'Amazon Javelin',
    'h2h2': 'Claw', 'glov': 'Gloves', 'boot': 'Boots', 'belt': 'Belt', 'ring': 'Ring', 'amul': 'Amulet',
    'cloa': 'Cloak',
}


def type_name(code):
    return TYPE_NAMES.get(code) or (ITYPES[code]['ItemType'] if code in ITYPES else code)


def ancestors(code, seen=None):
    seen = set() if seen is None else seen
    if code and code not in seen:
        seen.add(code)
        t = ITYPES.get(code)
        if t:
            ancestors(t['Equiv1'], seen)
            ancestors(t['Equiv2'], seen)
    return seen


def socket_category(code):
    """Which of a rune's gems.txt mods (weapon, helm or shield) an item type gets."""
    a = ancestors(code)
    return 'weapon' if 'weap' in a else 'shield' if 'shld' in a or 'shie' in a else 'helm'


CATEGORY_LABELS = {'weapon': 'in weapons', 'helm': 'in armor', 'shield': 'in shields'}
RUNE_REQ = {code: num(b['levelreq']) for code, b in BASES.items() if re.fullmatch(r'r\d\d', code)}

for idx, r in enumerate(table('runes')):
    row = idx + RUNEWORD_ROW_OFFSET
    if ('runeword', row) not in ITEMS:
        continue
    check('runeword', row, s(r['Name']) if s(r['Name']) != r['Name'] else r['*Rune Name'])
    runes = [r[f'Rune{i}'] for i in range(1, 7) if r[f'Rune{i}']]
    itypes = [r[f'itype{i}'] for i in range(1, 7) if r[f'itype{i}']]
    excluded = ', '.join(type_name(r[f'etype{i}']) for i in range(1, 4) if r[f'etype{i}'])
    emit('runeword', row, 'req', max(RUNE_REQ.get(x, 0) for x in runes))
    emit('runeword', row, 'sockets', len(runes))
    emit('runeword', row, 'bases', ', '.join(map(type_name, itypes)) + (f' (not {excluded})' if excluded else ''))
    own = [(r[f'T1Code{i}'], r[f'T1Param{i}'], num(r[f'T1Min{i}']), num(r[f'T1Max{i}']))
           for i in range(1, 8) if r[f'T1Code{i}']]
    # The runes add their own socket bonuses, which depend on the base (weapon, armor or shield);
    # lines that differ between the allowed bases are labelled with the base.
    by_cat = {}
    for code in itypes:
        cat = socket_category(code)
        mods = [(GEMS[x][f'{cat}Mod{j}Code'], GEMS[x][f'{cat}Mod{j}Param'], num(GEMS[x][f'{cat}Mod{j}Min']),
                 num(GEMS[x][f'{cat}Mod{j}Max'])) for x in runes for j in range(1, 4) if GEMS[x][f'{cat}Mod{j}Code']]
        by_cat.setdefault(cat, lines(own + mods))
    variants = list(by_cat.values())
    for t in variants[0]:
        if all(t in v for v in variants):
            emit('runeword', row, 'stat', t)
    for cat, ls in by_cat.items():
        for t in ls:
            if not all(t in v for v in variants):
                emit('runeword', row, 'stat', f'({CATEGORY_LABELS[cat]}) {t}')

covered = {(l.split('\t')[0], int(l.split('\t')[1])) for l in out if not l.startswith('setbonus')}
missing = sorted(k for k in ITEMS if k not in covered)
with open(OUT, 'w', encoding='utf-8', newline='\n') as f:
    f.write("# Item details for D2R v105 (Reign of the Warlock), generated by tools/gen_item_details.py from\n")
    f.write('# the game\'s excel tables (uniqueitems, setitems, sets, runes, properties, itemstatcost, ...)\n')
    f.write('# and English strings.\n')
    f.write('# kind\tkey\tfield\tvalue  (key: Chronicle row, or set name for setbonus)\n')
    f.write('# fields: req (level), stat, bonus (set item partial bonus), sockets, bases, partial/full (set bonuses)\n')
    for l in out:
        f.write(l + '\n')
for r in renamed:
    print('name differs:', r, file=sys.stderr)
print(f'wrote {len(out)} lines to {OUT}', file=sys.stderr)
if missing:
    sys.exit(f'no details for {missing}')
