// Observe ordinary fission through read-only WASM projections. This world starts
// empty, seeds once, and never inserts cells, so births allocate successive IDs.
import assert from 'node:assert/strict';
import { body, error, graph, memory } from './organelle-harness.mjs';

export function observePredatorBirths(api, founders, lineage) {
  const births = [];
  let previousSplits = [];
  const registers = new Map();
  let seen = 0;
  function cell(id) {
    const ptr = api.cell_snapshot_ptr(id);
    return ptr ? new Float64Array(api.memory.buffer, ptr, 11) : null;
  }
  function display(id) {
    const row = cell(id);
    if (!row) return null;
    const genome = row[1];
    if (!registers.has(genome)) registers.set(genome, graph(api, genome).contactMessages?.publish ?? null);
    const register = registers.get(genome);
    const marker = register ? memory(api, id).find(entry => entry.name === register)?.value ?? null : null;
    const assemblies = body(api, id).cellular?.assemblies ?? [];
    const patches = assemblies.filter(a => a.kind === 'display()' && a.functionalMass > 0 && a.activity > 0).length;
    return {register, marker, activeDisplayPatches: patches};
  }
  return {
    births,
    advance(ticks) {
      for (let i = 0; i < ticks; i++) {
        assert.equal(api.tick(1), 1, error(api));
        for (const entry of previousSplits) {
          entry.parentAfterOneTick = display(entry.parentId);
          entry.daughterAfterOneTick = display(entry.daughterId);
        }
        previousSplits = [];
        const ptr = api.summary_snapshot_ptr();
        const summary = new Float64Array(api.memory.buffer, ptr, 7);
        const count = summary[3], tick = summary[6];
        for (; seen < count; seen++) {
          const daughterId = founders + seen + 1;
          const row = cell(daughterId);
          // A daughter may deplete her stores during the same tick as fission.
          // The census records such births even when no live body can be inspected.
          if (!row) continue;
          assert.equal(row[0], daughterId);
          const parentId = row[10], daughterGenome = row[1];
          if (lineage(daughterGenome) !== 'Predator') continue;
          const entry = {tick, parentId, daughterId, parentAtSplit: display(parentId), daughterAtSplit: display(daughterId)};
          births.push(entry);
          previousSplits.push(entry);
        }
      }
    }
  };
}
