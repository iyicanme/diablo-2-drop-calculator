"""Pulls the excel tables and English strings that gen_item_details.py and gen_area_monsters.py need
out of a D2R install.

usage: py -I tools/extract_d2r.py <D2R data dir> <out dir>

The Steam build of D2R stores its files in data/*.data as back-to-back BLTE blobs with no index or
file names, so tables are recognised by their header line and string files by keys they contain.
Several patch generations of a table can be present. A later archive wins; within one archive the
largest copy wins (the others are older or classic-mode variants, e.g. an armor.txt without the
Warlock's grimoires), and of equal ones the first.
"""
import glob, json, mmap, os, struct, sys, zlib

TABLES = {  # header prefix -> file name
    'index\t*ID\tversion\tdisabled\t': 'uniqueitems.txt',
    'index\t*ID\tset\t': 'setitems.txt',
    'index\tname\tversion\tPCode2a\t': 'sets.txt',
    'Name\t*Rune Name\t': 'runes.txt',
    'code\t*Id\t*Enabled\tfunc1\t': 'properties.txt',
    'code\t*Id\tPickMode\t': 'propertygroups.txt',
    'Stat\t*ID\t': 'itemstatcost.txt',
    'ItemType\tCode\t': 'itemtypes.txt',
    'name\ttype\ttype2\tcode\t': 'weapons.txt',
    'name\tversion\tcompactsave\t': 'armor.txt',
    'name\tcompactsave\tversion\t': 'misc.txt',
    'name\tletter\ttransform\tcode\t': 'gems.txt',
    'skill\t*Id\tcharclass\t': 'skills.txt',
    'skilldesc\tSkillPage\t': 'skilldesc.txt',
    'Id\t*hcIdx\tBaseId\t': 'monstats.txt',
    'class\tstr\tdex\t': 'charstats.txt',
    'Name\t*StringName\tId\tPal\t': 'levels.txt',
    'Name\tLevel\tRooms\t': 'lvlmaze.txt',
    'Name\tDef\tLevelId\t': 'lvlprest.txt',
    'uniquemod\tid\tenabled\t': 'monumod.txt',
    'Superunique\tName\tClass\t': 'superuniques.txt',
}
STRINGS = {  # a key only that string file has -> file name
    'ModStr1a': 'item-modifiers.json',
    'Harlequin Crest': 'item-names.json',
    'Runeword22': 'item-runes.json',
    'Eskillname0': 'skills.json',
    'Low Quality': 'item-nameaffixes.json',
}

def blobs(path):
    """Yields (offset, mode, chunks) for each BLTE blob of an archive; chunks are (data, size)."""
    with open(path, 'rb') as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as m:
        pos = 0
        while pos < len(m) - 12:
            if m[pos:pos + 4] != b'BLTE':
                pos = m.find(b'BLTE', pos + 1)
                if pos < 0:
                    return
                continue
            header = struct.unpack('>I', m[pos + 4:pos + 8])[0]
            if header == 0:  # single unsized chunk; never seen in D2R, skip it
                pos += 8
                continue
            count = int.from_bytes(m[pos + 9:pos + 12], 'big')
            sizes = [struct.unpack('>I', m[pos + 12 + 24 * i:pos + 16 + 24 * i])[0] for i in range(count)]
            data = pos + header
            chunks, p = [], data
            for size in sizes:
                chunks.append((m, p, size))
                p += size
            yield pos, chunks
            pos = p

def chunk_bytes(m, p, size, limit=None):
    mode, body = m[p:p + 1], m[p + 1:p + size]
    if mode == b'N':
        return bytes(body[:limit] if limit else body)
    if mode == b'Z':
        d = zlib.decompressobj()
        return d.decompress(body, limit) if limit else d.decompress(body) + d.flush()
    return None  # encrypted or unknown encoding

def main():
    src, out = sys.argv[1:3]
    os.makedirs(out, exist_ok=True)
    found = {}  # file name -> ((archive, offset), bytes)
    for path in sorted(glob.glob(os.path.join(src, '*.data'))):
        archive = os.path.basename(path)
        for offset, chunks in blobs(path):
            try:
                head = chunk_bytes(*chunks[0], limit=400)
            except zlib.error:
                continue
            if not head:
                continue
            head = head.removeprefix(b'\xef\xbb\xbf')
            name = next((n for p, n in TABLES.items() if head.startswith(p.encode())), None)
            if name is None and not (head[:1] == b'[' and b'"Key"' in head):
                continue
            parts = [chunk_bytes(*c) for c in chunks]
            if None in parts:
                continue
            data = b''.join(parts)
            if name is None:
                keys = {e.get('Key') for e in json.loads(data.decode('utf-8-sig'))}
                name = next((n for k, n in STRINGS.items() if k in keys), None)
                if name is None:
                    continue
            old = found.get(name)
            if old is None or old[0][0] != archive or len(data) > len(old[1]):
                found[name] = ((archive, offset), data)
        print(archive, len(found), 'files so far', flush=True)
    for name, (where, data) in sorted(found.items()):
        with open(os.path.join(out, name), 'wb') as f:
            f.write(data)
        print(f'{name}: {where[0]} @ {where[1]}')
    missing = (set(TABLES.values()) | set(STRINGS.values())) - set(found)
    if missing:
        sys.exit(f'not found: {", ".join(sorted(missing))}')

main()
