// Runs the Rust engine (drop_calc.wasm) off the main thread and ships the columnar result back.
// A fresh instance per run means the previous run's linear memory is released.

const modulePromise = WebAssembly.compileStreaming(fetch('drop_calc.wasm')).catch(async () => {
  const response = await fetch('drop_calc.wasm');
  return WebAssembly.compile(await response.arrayBuffer());
});

// Column order matches src/wasm.rs.
const STRING_COLUMNS = ['monster', 'location', 'difficulty', 'item', 'tc', 'itemType', 'tier', 'quality', 'monsterType'];
const ACT = 9, FLAGS = 10, CATEGORY = 11, CHANCE = 12, DICTIONARY = 13;

self.onmessage = async ({ data: { players, magicFind, characterLevel } }) => {
  try {
    const instance = await WebAssembly.instantiate(await modulePromise, {});
    const wasm = instance.exports;
    const started = performance.now();
    // Last argument 0 = no rarity cutoff; the table's "1 in X" filter covers that.
    const rows = wasm.generate(players, magicFind, characterLevel, 0) >>> 0;
    if (rows === 0xffffffff) throw new Error('The calculator rejected these inputs.');

    const copy = (column) => {
      const ptr = wasm.column_ptr(column);
      return wasm.memory.buffer.slice(ptr, ptr + wasm.column_len(column));
    };
    const columns = {};
    STRING_COLUMNS.forEach((name, i) => (columns[name] = new Uint16Array(copy(i))));
    columns.act = new Uint8Array(copy(ACT));
    columns.flags = new Uint8Array(copy(FLAGS));
    columns.category = new Uint8Array(copy(CATEGORY));
    columns.chance = new Float64Array(copy(CHANCE));
    const dictionary = JSON.parse(new TextDecoder().decode(copy(DICTIONARY)));
    wasm.free_result();

    const elapsed = performance.now() - started;
    self.postMessage({ rows, columns, dictionary, elapsed }, Object.values(columns).map((c) => c.buffer));
  } catch (err) {
    self.postMessage({ error: String(err && err.message ? err.message : err) });
  }
};
