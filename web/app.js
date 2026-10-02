// Drop table UI: inputs -> worker (wasm) -> columnar rows -> filtered/sorted index view -> virtual table.
// Everything works on row indices into typed arrays; strings are u16 indices into `data.dictionary`.

const ROW_H = 28;
const OVERSCAN = 8;
// Browsers cap element height (~17.9M px in Firefox), so very long views scroll a capped spacer
// and map its position onto the full virtual height.
const MAX_SPACER_H = 8_000_000;
const PERM_CACHE_SIZE = 4;

const CATEGORIES = [
  { id: 2, label: 'Uniques & Sets', file: 'uniques_sets' },
  { id: 0, label: 'Base Items', file: 'base_items' },
  { id: 1, label: 'Consumables, Runes & Gems', file: 'consumables_runes_gems' },
  { id: -1, label: 'All', file: 'drops' },
];

// A subset of the CLI's CSV columns, in its order and with its headers (src/export.rs). Immunities
// are filtered from the header bar instead; treasure class and raw drop chance aren't shown.
// `width` is the minimum in px; columns with `grow` share the leftover table width in that ratio,
// so the table fits the window and only scrolls sideways below the sum of the minimums.
const COLUMNS = [
  { id: 'monster', label: 'Monster', type: 'text', width: 150, grow: 3 },
  { id: 'monsterType', label: 'Monster Type', csv: 'Monster Type', type: 'enum', width: 116 },
  { id: 'location', label: 'Location', type: 'text', width: 150, grow: 3 },
  { id: 'act', label: 'Act', type: 'enum', width: 62, align: 'num' },
  { id: 'difficulty', label: 'Difficulty', type: 'enum', width: 94 },
  { id: 'terrorized', label: 'Terrorized', type: 'flag', bit: 0, width: 90 },
  { id: 'item', label: 'Item', type: 'text', width: 170, grow: 3.5 },
  { id: 'itemType', label: 'Item Type', type: 'enum', width: 110, grow: 1.5 },
  { id: 'tier', label: 'Tier', type: 'enum', width: 100, grow: 0.5 },
  { id: 'quality', label: 'Quality', type: 'enum', width: 88 },
  { id: 'perX', label: '1 in X', csv: 'Chance (1 in X)', type: 'max', width: 130, grow: 1, align: 'num' },
];

// Bits 1-6 of the `flags` column (src/wasm.rs).
const IMMUNITIES = ['fire', 'cold', 'lightning', 'poison', 'magic', 'physical'];
const STRING_COLUMN_IDS = new Set(['monster', 'monsterType', 'location', 'difficulty', 'item', 'tc', 'itemType', 'tier', 'quality']);

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
const fmtInt = new Intl.NumberFormat();
const fmtPerX = new Intl.NumberFormat(undefined, { minimumFractionDigits: 1, maximumFractionDigits: 1 });

const $ = (sel) => document.querySelector(sel);
const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

// ---- state ---------------------------------------------------------------------------------------

let data = null; // { rows, columns, dictionary, dictRank, elapsed, inputs }
let tab = 2;
let sort = { id: null, dir: 1 }; // dir 1 asc, -1 desc
// text: { text, selected: Set<dict index> | null }, enum/flag: string ('' = all), max: string
const filters = {};
let excludedImmunities = 0; // bitmask over `flags`; rows with any of these bits are hidden
let statFilter = ''; // text searched in item stat lines, set bonuses and rune/gem socket bonuses
let view = new Uint32Array(0);
let viewBuf = new Uint32Array(0);
let categoryCounts = [0, 0, 0];
const permCache = new Map();
const optionCache = new Map(); // `${column}:${tab}` -> dict indices present in that tab

// ---- inputs & worker -----------------------------------------------------------------------------

const worker = new Worker('worker.js');
const form = $('#inputs');
const INPUT_IDS = ['players', 'magicFind', 'characterLevel'];

try {
  const saved = JSON.parse(localStorage.getItem('drop_calc.inputs') || 'null');
  if (saved) for (const id of INPUT_IDS) if (saved[id] != null) form.elements[id].value = saved[id];
} catch {}

// Header: one row when the inputs and both live filters fit, else stack them all vertically.
const header = $('.top');
function layoutHeader() {
  header.classList.remove('stacked');
  if (header.scrollWidth > header.clientWidth + 1) header.classList.add('stacked');
}
new ResizeObserver(layoutHeader).observe(header);

const immunityBoxes = [...document.querySelectorAll('#immunities input[type=checkbox]')];
try {
  const saved = JSON.parse(localStorage.getItem('drop_calc.immunities') || '[]');
  for (const box of immunityBoxes) box.checked = saved.includes(box.name);
} catch {}
function readImmunities() {
  excludedImmunities = 0;
  immunityBoxes.forEach((box) => {
    if (box.checked) excludedImmunities |= 1 << (IMMUNITIES.indexOf(box.name) + 1);
  });
}
readImmunities();
for (const box of immunityBoxes) {
  box.addEventListener('change', () => {
    readImmunities();
    try {
      localStorage.setItem('drop_calc.immunities', JSON.stringify(immunityBoxes.filter((b) => b.checked).map((b) => b.name)));
    } catch {}
    refresh();
  });
}

form.addEventListener('submit', (e) => {
  e.preventDefault();
  if (!form.reportValidity()) return;
  const inputs = {
    players: Number(form.elements.players.value),
    magicFind: Number(form.elements.magicFind.value),
    characterLevel: Number(form.elements.characterLevel.value),
  };
  try {
    localStorage.setItem('drop_calc.inputs', JSON.stringify(Object.fromEntries(INPUT_IDS.map((id) => [id, form.elements[id].value]))));
  } catch {}
  setBusy(true, 'Calculating drop tables…');
  worker.postMessage(inputs);
  worker.inputs = inputs;
});

worker.onmessage = ({ data: msg }) => {
  if (msg.error) {
    setBusy(false);
    setStatus(`Error: ${msg.error}`, true);
    return;
  }
  const order = msg.dictionary.map((_, i) => i).sort((a, b) => collator.compare(msg.dictionary[a], msg.dictionary[b]));
  const dictRank = new Uint16Array(msg.dictionary.length);
  order.forEach((d, rank) => (dictRank[d] = rank));
  const categoryTotals = [0, 0, 0];
  for (const c of msg.columns.category) categoryTotals[c]++;
  // Lowercased stat lines per item for the item-stats filter. Plain base items have none.
  const itemStats = msg.details.items.map((it) => statLines(it).join('\n').toLowerCase());
  data = { ...msg, dictRank, categoryTotals, itemStats, inputs: worker.inputs };
  permCache.clear();
  optionCache.clear();
  closePicker();
  hideCard();
  viewBuf = new Uint32Array(data.rows);
  buildFilterRow();
  setBusy(false);
  refresh();
};
worker.onerror = (e) => {
  setBusy(false);
  setStatus(`Error: ${e.message || 'failed to load the calculator (serve this folder over http, not file://)'}`, true);
};

function setBusy(busy, message) {
  $('#run').disabled = busy;
  document.body.classList.toggle('busy', busy);
  if (message) setStatus(message);
}
function setStatus(text, isError = false) {
  const s = $('#status');
  s.textContent = text;
  s.classList.toggle('error', isError);
}

// ---- table skeleton ------------------------------------------------------------------------------

const scroller = $('#scroller');
const table = $('#table');
const headRow = $('#head-row');
const filterRow = $('#filter-row');
const body = $('#body');
const spacer = $('#spacer');
const emptyMsg = $('#empty');

const template = COLUMNS.map((c) => (c.grow ? `minmax(${c.width}px, ${c.grow}fr)` : `${c.width}px`)).join(' ');
table.style.setProperty('--cols', template);
table.style.minWidth = `${COLUMNS.reduce((s, c) => s + c.width, 0)}px`;

for (const c of COLUMNS) {
  const h = el('button', `th ${c.align || ''}`);
  h.type = 'button';
  h.title = `Sort by ${c.csv || c.label}`;
  h.append(el('span', 'th-label', c.label), el('span', 'sort-ind'));
  h.addEventListener('click', () => onSort(c.id));
  h.dataset.id = c.id;
  headRow.append(h);
}

function debounce(fn, ms) {
  let t;
  return (...a) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
}
const refreshSoon = debounce(() => refresh(), 180);

function buildFilterRow() {
  filterRow.replaceChildren();
  for (const c of COLUMNS) {
    const cell = el('div', 'fcell');
    if (c.type === 'text') {
      const f = textFilter(c.id);
      const input = el('input', 'finput');
      input.type = 'search';
      input.placeholder = 'contains…';
      input.title = 'Case-insensitive; separate alternatives with commas';
      input.value = f.text;
      input.addEventListener('input', () => {
        f.text = input.value;
        refreshSoon();
      });
      const button = el('button', 'pick');
      button.type = 'button';
      button.title = `Choose ${c.label.toLowerCase()} values from a list`;
      button.dataset.id = c.id;
      button.addEventListener('click', () => (picker?.id === c.id ? closePicker() : openPicker(c, cell)));
      cell.classList.add('with-pick');
      cell.append(input, button);
    } else if (c.type === 'enum' || c.type === 'flag') {
      const select = el('select', 'finput');
      select.append(new Option('All', ''));
      for (const [value, label] of enumOptions(c)) select.append(new Option(label, value));
      select.value = filters[c.id] || '';
      if (select.value !== (filters[c.id] || '')) filters[c.id] = '';
      select.addEventListener('change', () => {
        filters[c.id] = select.value;
        refresh();
      });
      cell.append(select);
    } else {
      const input = el('input', 'finput');
      input.type = 'number';
      input.step = 'any';
      input.min = '1';
      input.placeholder = 'at most…';
      input.title = 'Hide drops rarer than 1 in this many';
      input.value = filters[c.id] || '';
      input.addEventListener('input', () => {
        filters[c.id] = input.value;
        refreshSoon();
      });
      cell.append(input);
    }
    filterRow.append(cell);
  }
  for (const c of COLUMNS) if (c.type === 'text') updatePickButton(c.id);
}

function enumOptions(c) {
  if (c.type === 'flag') return [['1', 'yes'], ['0', 'no']];
  if (c.id === 'act') {
    const seen = new Set(data.columns.act);
    return [...seen].sort((a, b) => a - b).map((a) => [String(a), String(a)]);
  }
  return distinctValues(c.id, -1).map((d) => [String(d), data.dictionary[d]]);
}

/** Dictionary indices occurring in string column `id` (only in category `inTab` unless -1), sorted. */
function distinctValues(id, inTab) {
  const col = data.columns[id];
  const category = data.columns.category;
  const seen = new Uint8Array(data.dictionary.length);
  for (let r = 0; r < col.length; r++) if (inTab === -1 || category[r] === inTab) seen[col[r]] = 1;
  const values = [];
  for (let d = 0; d < seen.length; d++) if (seen[d]) values.push(d);
  return values.sort((a, b) => data.dictRank[a] - data.dictRank[b]);
}

const textFilter = (id) => (filters[id] ||= { text: '', selected: null });

// ---- checkbox-list picker for text columns -------------------------------------------------------

let picker = null; // { id, el, anchor }

function updatePickButton(id) {
  const button = filterRow.querySelector(`.pick[data-id="${id}"]`);
  if (!button) return;
  const n = filters[id]?.selected?.size || 0;
  button.textContent = n ? `▾ ${fmtInt.format(n)}` : '▾';
  button.classList.toggle('active', n > 0);
  button.setAttribute('aria-expanded', String(picker?.id === id));
}

function openPicker(c, anchor) {
  closePicker();
  const f = textFilter(c.id);
  const { dictionary } = data;
  let values = optionCache.get(`${c.id}:${tab}`);
  if (!values) optionCache.set(`${c.id}:${tab}`, (values = distinctValues(c.id, tab)));

  const pop = el('div', 'picker');
  pop.setAttribute('role', 'dialog');
  pop.setAttribute('aria-label', `${c.label} values`);
  const search = el('input', 'finput');
  search.type = 'search';
  search.placeholder = `Search ${fmtInt.format(values.length)} values…`;
  const selectAll = el('button', 'secondary', 'Select all');
  const clear = el('button', 'secondary', 'Clear');
  selectAll.type = clear.type = 'button';
  const actions = el('div', 'picker-actions');
  actions.append(selectAll, clear);
  const list = el('div', 'picker-list');

  const entries = values.map((d) => {
    const label = el('label', 'picker-item');
    const box = el('input');
    box.type = 'checkbox';
    box.checked = !!f.selected?.has(d);
    box.dataset.d = d;
    label.append(box, el('span', null, dictionary[d]));
    return { d, label, box, lower: dictionary[d].toLowerCase() };
  });
  list.append(...entries.map((e) => e.label));

  const changed = () => {
    if (f.selected && !f.selected.size) f.selected = null;
    updatePickButton(c.id);
    refreshSoon();
  };
  list.addEventListener('change', (e) => {
    const d = Number(e.target.dataset.d);
    if (e.target.checked) (f.selected ||= new Set()).add(d);
    else f.selected?.delete(d);
    changed();
  });
  search.addEventListener('input', () => {
    const q = search.value.trim().toLowerCase();
    for (const e of entries) e.label.hidden = q !== '' && !e.lower.includes(q);
    selectAll.textContent = q ? 'Select shown' : 'Select all';
  });
  selectAll.addEventListener('click', () => {
    for (const e of entries) {
      if (e.label.hidden) continue;
      e.box.checked = true;
      (f.selected ||= new Set()).add(e.d);
    }
    changed();
  });
  clear.addEventListener('click', () => {
    for (const e of entries) e.box.checked = false;
    f.selected = null;
    changed();
  });

  pop.append(search, actions, list);
  document.body.append(pop);
  picker = { id: c.id, el: pop, anchor };
  positionPicker();
  updatePickButton(c.id);
  search.focus();
}

function positionPicker() {
  if (!picker) return;
  const rect = picker.anchor.getBoundingClientRect();
  const width = picker.el.offsetWidth;
  picker.el.style.left = `${Math.max(8, Math.min(rect.left, window.innerWidth - width - 8))}px`;
  picker.el.style.top = `${rect.bottom + 4}px`;
}

function closePicker() {
  if (!picker) return;
  const { id } = picker;
  picker.el.remove();
  picker = null;
  updatePickButton(id);
}

document.addEventListener('mousedown', (e) => {
  if (picker && !picker.el.contains(e.target) && !picker.anchor.contains(e.target)) closePicker();
});
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    closePicker();
    hideCard();
  }
});

// ---- filtering -----------------------------------------------------------------------------------

/** Every searchable line of an item: own stats, set bonuses, and what a rune/gem adds when socketed. */
function statLines(it) {
  if (!it) return [];
  return [
    ...(it.props || []),
    ...(it.setBonuses || []).map((b) => b.text),
    ...(it.socketed || []).flatMap((s) => s.lines),
  ];
}

/** Comma-separated, case-insensitive alternatives. */
const splitTerms = (s) => s.split(',').map((t) => t.trim().toLowerCase()).filter(Boolean);

const statInput = $('#stat-filter');
statInput.addEventListener('input', () => {
  statFilter = statInput.value;
  refreshSoon();
});

function buildTests() {
  const { columns, dictionary } = data;
  const tests = [];
  const num = (s) => (s === '' || s == null ? null : Number(s));

  for (const c of COLUMNS) {
    const f = filters[c.id];
    if (c.type === 'text') {
      if (!f) continue;
      const terms = splitTerms(f.text);
      if (!terms.length && !f.selected) continue;
      // Text and checkbox selection both narrow the column (AND).
      const mask = new Uint8Array(dictionary.length);
      dictionary.forEach((s, d) => {
        if (f.selected && !f.selected.has(d)) return;
        const lower = s.toLowerCase();
        if (!terms.length || terms.some((t) => lower.includes(t))) mask[d] = 1;
      });
      const col = columns[c.id];
      tests.push((r) => mask[col[r]] === 1);
    } else if (c.type === 'enum') {
      if (!f) continue;
      const v = Number(f);
      const col = c.id === 'act' ? columns.act : columns[c.id];
      tests.push((r) => col[r] === v);
    } else if (c.type === 'flag') {
      if (!f) continue;
      const want = Number(f);
      const flags = columns.flags;
      const bit = c.bit;
      tests.push((r) => ((flags[r] >> bit) & 1) === want);
    } else if (c.type === 'max') {
      const max = num(f);
      if (max == null || !(max > 0)) continue;
      // Compare the value as displayed (1 decimal), so a row showing "1,000.0" passes "1000".
      const chance = columns.chance;
      tests.push((r) => Math.round(10 / chance[r]) / 10 <= max);
    }
  }
  const statTerms = splitTerms(statFilter);
  if (statTerms.length) {
    const mask = new Uint8Array(data.itemStats.length);
    data.itemStats.forEach((text, i) => {
      if (statTerms.some((t) => text.includes(t))) mask[i] = 1;
    });
    const itemRef = columns.itemRef;
    tests.push((r) => mask[itemRef[r]] === 1);
  }
  if (excludedImmunities) {
    const flags = columns.flags;
    const mask = excludedImmunities;
    tests.push((r) => (flags[r] & mask) === 0);
  }
  return tests;
}

function refresh() {
  if (!data) return render();
  const n = data.rows;
  const perm = sort.id ? getPerm(sort.id) : null;
  const tests = buildTests();
  const nTests = tests.length;
  const category = data.columns.category;
  const counts = [0, 0, 0];
  let count = 0;
  const reverse = sort.id && effectiveDir() < 0;

  for (let i = 0; i < n; i++) {
    const r = perm ? perm[reverse ? n - 1 - i : i] : i;
    let ok = true;
    for (let t = 0; t < nTests; t++) {
      if (!tests[t](r)) {
        ok = false;
        break;
      }
    }
    if (!ok) continue;
    const cat = category[r];
    counts[cat]++;
    if (tab === -1 || cat === tab) viewBuf[count++] = r;
  }
  view = viewBuf.subarray(0, count);
  categoryCounts = counts;
  render(true);
}

// ---- sorting -------------------------------------------------------------------------------------

// "1 in X" is just the drop chance inverted, so it shares the chance permutation.
const permKey = (id) => (id === 'perX' ? 'chance' : id);
const effectiveDir = () => (sort.id === 'perX' ? -sort.dir : sort.dir);

function onSort(id) {
  if (!data) return;
  if (sort.id !== id) sort = { id, dir: 1 };
  else if (sort.dir === 1) sort = { id, dir: -1 };
  else sort = { id: null, dir: 1 };
  const key = sort.id && permKey(sort.id);
  if (key && !permCache.has(key)) {
    setStatus('Sorting…');
    document.body.classList.add('busy');
    // Let the status paint before the (sub-second) sort blocks the thread.
    requestAnimationFrame(() =>
      setTimeout(() => {
        document.body.classList.remove('busy');
        refresh();
      }, 0),
    );
  } else {
    refresh();
  }
}

function getPerm(id) {
  const key = permKey(id);
  let perm = permCache.get(key);
  if (perm) {
    permCache.delete(key);
    permCache.set(key, perm); // mark most recently used
    return perm;
  }
  const { columns, dictRank, rows: n } = data;
  const c = COLUMNS.find((c) => c.id === key);
  const ranks = new Float64Array(n);
  if (STRING_COLUMN_IDS.has(key)) {
    const col = columns[key];
    for (let r = 0; r < n; r++) ranks[r] = dictRank[col[r]];
  } else if (key === 'act') {
    ranks.set(columns.act);
  } else if (c?.type === 'flag') {
    const flags = columns.flags;
    for (let r = 0; r < n; r++) ranks[r] = (flags[r] >> c.bit) & 1;
  } else {
    // Arbitrary doubles: replace each with its rank among the distinct values.
    const uniq = columns.chance.slice().sort();
    let u = 0;
    for (let i = 0; i < n; i++) if (u === 0 || uniq[i] !== uniq[u - 1]) uniq[u++] = uniq[i];
    const values = columns.chance;
    for (let r = 0; r < n; r++) {
      let a = 0;
      let b = u - 1;
      const v = values[r];
      while (a < b) {
        const m = (a + b) >> 1;
        if (uniq[m] < v) a = m + 1;
        else b = m;
      }
      ranks[r] = a;
    }
  }
  // Pack (rank, row) into one double so a native typed-array sort does the work; row as the low
  // part keeps ties in generation order (a stable sort).
  const shift = 2 ** Math.max(1, Math.ceil(Math.log2(n + 1)));
  for (let r = 0; r < n; r++) ranks[r] = ranks[r] * shift + r;
  ranks.sort();
  perm = new Uint32Array(n);
  for (let i = 0; i < n; i++) perm[i] = ranks[i] % shift;

  permCache.set(key, perm);
  if (permCache.size > PERM_CACHE_SIZE) permCache.delete(permCache.keys().next().value);
  return perm;
}

// ---- rendering -----------------------------------------------------------------------------------

const rowPool = [];

function cellText(c, r) {
  const { columns, dictionary } = data;
  if (STRING_COLUMN_IDS.has(c.id)) return dictionary[columns[c.id][r]];
  switch (c.id) {
    case 'act':
      return String(columns.act[r]);
    case 'perX':
      return columns.chance[r] > 0 ? fmtPerX.format(1 / columns.chance[r]) : '';
    default:
      return (columns.flags[r] >> c.bit) & 1 ? 'yes' : 'no';
  }
}

function render(resetScroll = false) {
  renderChrome();
  const count = view.length;
  emptyMsg.hidden = count > 0;
  emptyMsg.textContent = !data ? 'Set the inputs above and press Calculate.' : 'No rows match the current filters.';
  const virtualH = count * ROW_H;
  spacer.style.height = `${Math.min(virtualH, MAX_SPACER_H)}px`;
  if (resetScroll) scroller.scrollTop = 0;
  renderRows();
}

function renderRows() {
  const count = view.length;
  const virtualH = count * ROW_H;
  const spacerH = Math.min(virtualH, MAX_SPACER_H);
  const viewportH = Math.max(0, scroller.clientHeight - body.offsetTop);
  const scrolled = Math.max(0, scroller.scrollTop);
  const ratio = spacerH > viewportH ? (virtualH - viewportH) / (spacerH - viewportH) : 1;
  const virtualTop = Math.min(scrolled * ratio, Math.max(0, virtualH - viewportH));
  const first = Math.max(0, Math.floor(virtualTop / ROW_H) - OVERSCAN);
  const last = Math.min(count, Math.ceil((virtualTop + viewportH) / ROW_H) + OVERSCAN);
  const needed = Math.max(0, last - first);

  while (rowPool.length < needed) {
    const row = el('div', 'tr');
    for (const c of COLUMNS) row.append(el('div', `td ${c.align || ''} col-${c.id}`));
    spacer.append(row);
    rowPool.push(row);
  }
  for (let p = 0; p < rowPool.length; p++) {
    const row = rowPool[p];
    if (p >= needed) {
      row.hidden = true;
      continue;
    }
    const i = first + p;
    const r = view[i];
    row.hidden = false;
    row.dataset.r = r;
    row.style.transform = `translateY(${scrolled + i * ROW_H - virtualTop}px)`;
    row.classList.toggle('odd', i % 2 === 1);
    const quality = data.dictionary[data.columns.quality[r]];
    const tier = data.dictionary[data.columns.tier[r]];
    row.dataset.quality = quality === 'Base Item' ? (tier === 'Rune' ? 'rune' : 'base') : quality.toLowerCase();
    const cells = row.children;
    for (let k = 0; k < COLUMNS.length; k++) {
      const c = COLUMNS[k];
      const text = cellText(c, r);
      const cell = cells[k];
      if (cell.textContent !== text) cell.textContent = text;
      if (c.type === 'flag') cell.classList.toggle('yes', text === 'yes');
    }
  }
}

function renderChrome() {
  for (const h of headRow.children) {
    const active = sort.id === h.dataset.id;
    h.classList.toggle('sorted', active);
    h.setAttribute('aria-sort', active ? (sort.dir > 0 ? 'ascending' : 'descending') : 'none');
    h.querySelector('.sort-ind').textContent = active ? (sort.dir > 0 ? '▲' : '▼') : '';
  }

  const tabs = $('#tabs');
  tabs.replaceChildren();
  for (const t of CATEGORIES) {
    const b = el('button', 'tab');
    b.type = 'button';
    b.setAttribute('role', 'tab');
    b.setAttribute('aria-selected', String(t.id === tab));
    const n = !data ? null : t.id === -1 ? categoryCounts[0] + categoryCounts[1] + categoryCounts[2] : categoryCounts[t.id];
    b.append(el('span', null, t.label));
    if (n != null) b.append(el('span', 'count', fmtInt.format(n)));
    b.addEventListener('click', () => {
      tab = t.id;
      closePicker();
      refresh();
    });
    tabs.append(b);
  }

  $('#download').disabled = !data || view.length === 0;
  $('#clear').disabled = !data;
  if (data && !document.body.classList.contains('busy')) {
    const total = tab === -1 ? data.rows : data.categoryTotals[tab];
    const { players, magicFind, characterLevel } = data.inputs;
    setStatus(
      `${fmtInt.format(view.length)} of ${fmtInt.format(total)} rows · ` +
        `${players} player${players > 1 ? 's' : ''}, ${magicFind}% MF, clvl ${characterLevel}` +
        (excludedImmunities
          ? ` · hiding ${IMMUNITIES.filter((_, i) => excludedImmunities & (1 << (i + 1))).join(', ')} immune`
          : '') +
        ` · computed in ${(data.elapsed / 1000).toFixed(1)}s`,
    );
  }
}

let scrollQueued = false;
scroller.addEventListener('scroll', () => {
  if (scrollQueued) return;
  scrollQueued = true;
  hideCard();
  requestAnimationFrame(() => {
    scrollQueued = false;
    renderRows();
    positionPicker();
  });
});
window.addEventListener('resize', () => {
  renderRows();
  positionPicker();
});

// ---- hover cards ---------------------------------------------------------------------------------

const HOVER_DELAY_MS = 350;
const HOVER_KINDS = { 'col-monster': monsterCard, 'col-location': locationCard, 'col-item': itemCard };
const RESISTANCES = ['Fire', 'Cold', 'Lightning', 'Poison', 'Magic', 'Physical'];
const MAX_LISTED_MONSTERS = 12;

const card = el('div', 'hovercard');
card.hidden = true;
card.setAttribute('role', 'tooltip');
document.body.append(card);
let hoverTimer = 0;
let hoverCell = null;

function hideCard() {
  clearTimeout(hoverTimer);
  hoverCell = null;
  card.hidden = true;
}

spacer.addEventListener('mouseover', (e) => {
  const cell = e.target.closest('.td');
  if (cell === hoverCell) return;
  hideCard();
  const build = cell && HOVER_KINDS[[...cell.classList].find((c) => c in HOVER_KINDS)];
  if (!build || !data || picker) return;
  hoverCell = cell;
  hoverTimer = setTimeout(() => {
    const r = Number(cell.parentElement.dataset.r);
    card.replaceChildren(...build(r));
    card.hidden = false;
    positionCard(cell);
  }, HOVER_DELAY_MS);
});
spacer.addEventListener('mouseleave', hideCard);

function positionCard(cell) {
  const rect = cell.getBoundingClientRect();
  const { offsetWidth: w, offsetHeight: h } = card;
  const left = Math.max(8, Math.min(rect.left, window.innerWidth - w - 8));
  // Below the cell, or above it when there's no room.
  const below = rect.bottom + 6;
  const top = below + h <= window.innerHeight - 8 ? below : Math.max(8, rect.top - h - 6);
  card.style.left = `${left}px`;
  card.style.top = `${top}px`;
}

const fmtNum = (n) => fmtInt.format(n);
const span = (a, b) => (a === b ? fmtNum(a) : `${fmtNum(a)}–${fmtNum(b)}`);

function cardHead(title, subtitle, cls) {
  const head = el('div', 'hc-head');
  head.append(el('div', `hc-title ${cls || ''}`, title));
  if (subtitle) head.append(el('div', 'hc-sub', subtitle));
  return head;
}

/** Label/value rows; entries with a null value are skipped. */
function facts(pairs) {
  const dl = el('dl', 'hc-facts');
  for (const [label, value] of pairs) {
    if (value == null || value === '') continue;
    dl.append(el('dt', null, label), el('dd', null, String(value)));
  }
  return dl;
}

function section(title, ...children) {
  const s = el('div', 'hc-section');
  if (title) s.append(el('div', 'hc-label', title));
  s.append(...children);
  return s;
}

function rowContext(r) {
  const { columns, dictionary } = data;
  return {
    difficulty: dictionary[columns.difficulty[r]],
    type: dictionary[columns.monsterType[r]],
    level: columns.level[r],
    terrorized: (columns.flags[r] & 1) === 1,
  };
}

function monsterCard(r) {
  const m = data.details.monsters[data.columns.monsterRef[r]];
  if (!m) return [el('div', 'hc-sub', 'No details available.')];
  const { difficulty, type, level, terrorized } = rowContext(r);
  const stats = m.stats[difficulty];
  const out = [cardHead(m.name, [type, m.base, `${difficulty}${terrorized ? ', terrorized' : ''}`].filter(Boolean).join(' · '))];

  const tags = el('div', 'hc-tags');
  for (const [on, label] of [[m.boss, 'Boss'], [m.undead, 'Undead'], [m.demon, 'Demon']]) if (on) tags.append(el('span', 'hc-tag', label));
  if (tags.childElementCount) out.push(tags);

  if (stats) {
    const base = data.details.monlvl[difficulty];
    const hpBase = base.hp[level] ?? 0;
    const xpBase = base.xp[level] ?? 0;
    out.push(
      facts([
        ['Monster level', level],
        ['Life', span(Math.floor((stats.hp[0] * hpBase) / 100), Math.floor((stats.hp[1] * hpBase) / 100))],
        ['Experience', fmtNum(Math.floor((stats.xp * xpBase) / 100))],
      ]),
    );
    out.push(el('div', 'hc-note', 'Life and experience: 1 player, before champion/unique bonuses.'));
    const grid = el('div', 'hc-res');
    stats.res.forEach((v, i) => {
      const cell = el('div', `hc-res-cell${v >= 100 ? ' immune' : ''}`);
      cell.append(el('span', null, RESISTANCES[i]), el('b', null, `${v}%`));
      grid.append(cell);
    });
    out.push(section('Resistances', grid));
  }
  return out;
}

function locationCard(r) {
  const a = data.details.areas[data.columns.areaRef[r]];
  if (!a) return [el('div', 'hc-sub', 'No details available.')];
  const { difficulty } = rowContext(r);
  const packs = a.uniquePacks[difficulty];
  const out = [
    cardHead(a.name, `Act ${a.act} · ${difficulty}`),
    facts([
      ['Area level', a.levels[difficulty]],
      ['Unique packs', packs && (packs[1] > 0 ? span(packs[0], packs[1]) : 'none')],
    ]),
  ];
  if (a.superuniques.length) out.push(section('Super uniques', el('div', 'hc-list', a.superuniques.join(', '))));
  const monsters = a.monsters[difficulty] || [];
  if (monsters.length) {
    const shown = monsters.slice(0, MAX_LISTED_MONSTERS).join(', ');
    const more = monsters.length > MAX_LISTED_MONSTERS ? ` +${monsters.length - MAX_LISTED_MONSTERS} more` : '';
    out.push(section('Monsters', el('div', 'hc-list', shown + more)));
  }
  return out;
}

function itemCard(r) {
  const it = data.details.items[data.columns.itemRef[r]];
  if (!it) return [el('div', 'hc-sub', 'No details available.')];
  const kindClass = it.kind === 'unique' ? 'q-unique' : it.kind === 'set' ? 'q-set' : '';
  const subtitle = it.kind === 'base' ? `${it.type} · ${it.tier}` : `${it.base} · ${it.type} · ${it.tier}`;
  const pair = (p) => p && span(p[0], p[1]);
  const out = [
    cardHead(it.name, subtitle, kindClass),
    facts([
      ['Set', it.set],
      ['Item level', it.qlvl || null], // 0 for runes/gems, which drop from their own TCs
      ['Required level', it.reqLevel],
      ['Defense', pair(it.defense)],
      ['One-hand damage', pair(it.damage1h)],
      ['Two-hand damage', pair(it.damage2h)],
      ['Required strength', it.reqStr],
      ['Required dexterity', it.reqDex],
      ['Durability', it.durability],
      ['Max sockets', it.sockets],
    ]),
  ];
  if (it.kind !== 'base') {
    out.push(el('div', 'hc-note', 'Base defense/damage shown; the item’s own bonuses are below.'));
  }
  // Lines matching the item-stats filter are emphasised.
  const terms = splitTerms(statFilter);
  const statList = (lines, cls) => {
    const list = el('ul', `hc-props ${cls || ''}`);
    for (const [line, label] of lines) {
      const lower = line.toLowerCase();
      list.append(el('li', terms.some((t) => lower.includes(t)) ? 'match' : null, label ?? line));
    }
    return list;
  };
  if (it.props?.length) out.push(section(null, statList(it.props.map((l) => [l]))));
  if (it.setBonuses?.length) {
    out.push(section('Set bonuses', statList(it.setBonuses.map((b) => [b.text, `${b.text} (${b.items} items)`]), 'set')));
  }
  for (const s of it.socketed || []) out.push(section(`Socketed in ${s.slot.toLowerCase()}`, statList(s.lines.map((l) => [l]))));
  return out;
}

// ---- toolbar -------------------------------------------------------------------------------------

$('#clear').addEventListener('click', () => {
  for (const k of Object.keys(filters)) delete filters[k];
  statFilter = statInput.value = '';
  closePicker();
  buildFilterRow();
  refresh();
});

$('#download').addEventListener('click', () => {
  if (!data || !view.length) return;
  const esc = (s) => (/[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);
  const chunks = [COLUMNS.map((c) => c.csv || c.label).join(',') + '\n'];
  const CHUNK = 20000;
  for (let start = 0; start < view.length; start += CHUNK) {
    const lines = [];
    const end = Math.min(view.length, start + CHUNK);
    for (let i = start; i < end; i++) {
      const r = view[i];
      // Match the CLI's CSV: plain numbers, no thousands separators.
      lines.push(COLUMNS.map((c) => (c.id === 'perX' ? (1 / data.columns.chance[r]).toFixed(1) : esc(cellText(c, r)))).join(','));
    }
    chunks.push(lines.join('\n') + '\n');
  }
  const url = URL.createObjectURL(new Blob(chunks, { type: 'text/csv' }));
  const a = el('a');
  a.href = url;
  a.download = `${CATEGORIES.find((t) => t.id === tab).file}.csv`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
});

render();
