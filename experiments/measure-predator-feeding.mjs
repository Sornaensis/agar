import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { bytes, catalog, compile, error, fresh, seed } from '../scripts/organelle-harness.mjs';

// Usage from the project folder:
// node --no-liftoff experiments/measure-predator-feeding.mjs examples/predator.cell 7 300 result.json
const [sourcePath, seedArg, durationArg, output] = process.argv.slice(2);
const worldSeed = Number(seedArg);
const duration = Number(durationArg);
assert.ok(sourcePath && output && Number.isInteger(worldSeed) && worldSeed >= 0 && worldSeed <= 0xffffffff
  && Number.isSafeInteger(duration) && duration > 0, 'Expected predator.cell, seed, seconds and output.json');
const { BUILTIN_SIMULATIONS, BUILTIN_GENOMES, BUILTIN_ENVIRONMENTS, simulationStartSettings } = await catalog();
const basin = BUILTIN_SIMULATIONS[0];
const habitat = BUILTIN_ENVIRONMENTS.find(env => env.id === basin.environmentId);
const api = await fresh({ ...habitat.config, ...simulationStartSettings(), seed:worldSeed });
const source = (await readFile(sourcePath, 'utf8')).trim();
const hash = data => createHash('sha256').update(data).digest('hex');
const report = { seed:worldSeed, founders:{}, engineSha256:hash(bytes), rows:[] };
let hunter;
for (const entry of basin.populations) {
  const genome = BUILTIN_GENOMES.find(item => item.id === entry.genomeId);
  const program = genome.name === 'Predator' ? source : genome.source;
  const id = compile(api,program);
  if (genome.name === 'Predator') hunter = id;
  report.founders[genome.name] = { count:entry.count, sha256:hash(program) };
  seed(api,id,entry.count);
}
assert.ok(hunter);
let assimilatedEnergy = 0, hunterSeconds = 0;
const hz = 30;
for (let tick = 1; tick <= duration * hz; tick++) {
  assert.equal(api.tick(1),1,error(api));
  const pointer = api.ecology_snapshot_ptr();
  const cells = new Float64Array(api.memory.buffer,pointer,api.presentation_len());
  const stride = 20;
  assert.equal(cells.length, 2 + cells[1] * stride);
  const census = tick % (30 * hz) === 0 || tick === duration * hz;
  let predator, living = 0, richness = 0;
  for (let row = 2; row < cells.length; row += stride) {
    if (cells[row] === hunter) {
      // Each post-tick rate sums actual assimilated food energy in living
      // members of this lineage; a wounded or dead prey alone contributes zero.
      assimilatedEnergy += cells[row+13] / hz;
      hunterSeconds += cells[row+1] / hz;
      if (census) predator = { alive:cells[row+1], births:cells[row+3], killedByPredation:cells[row+5],
        depletionDeaths:cells[row+6], kills:cells[row+8], gutEnergy:cells[row+18] };
    }
    if (census) {
      living += cells[row+1];
      richness += Number(cells[row+1] > 0);
    }
  }
  if (!census) continue;
  const ledger = new BigUint64Array(api.memory.buffer,api.count_audit_ptr(),12).slice();
  const losses = new BigInt64Array(api.memory.buffer,api.energy_loss_ptr(),7);
  for (const i of [0,3,6]) assert.equal(ledger[i]+ledger[i+2],ledger[i+1]);
  assert.equal(ledger[9]+ledger[11]+losses.reduce((a,b) => a+b,0n),ledger[10]);
  report.rows.push({ seconds:tick/hz, living, richness, ...predator, assimilatedEnergy, hunterSeconds,
    assimilationPerLivingHunterSecond:assimilatedEnergy/hunterSeconds });
}
await writeFile(output,JSON.stringify(report,null,2)+'\n');
