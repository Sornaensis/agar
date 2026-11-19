import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { advance, catalog, compile, fresh, seed, snapshot } from './organelle-harness.mjs';

// A predator introduced without prey lives only on its own stores. The paired
// Still water trial provides an actual photosynthetic founder cohort and sunlight.
const {BUILTIN_ENVIRONMENTS,BUILTIN_GENOMES,simulationStartSettings}=await catalog();
const water=BUILTIN_ENVIRONMENTS.find(item=>item.id==='builtin:environment:still-water').config;
const named=name=>BUILTIN_GENOMES.find(g=>g.name===name).source;
const args=process.argv.slice(2),compare=args.includes('--compare'),files=args.filter(a=>a!=='--compare');
assert.ok(files.length<=1 && files.every(p=>!p.startsWith('--')));
const revised=files.length?await readFile(files[0],'utf8'):named('Predator');
const cases=[['revised',revised]];
if(compare) cases.unshift(['former',await readFile(new URL('../experiments/fixtures/predator-before-curation.cell',import.meta.url),'utf8')]);
const rows=[];
for(const [label,source] of cases) for(const randomSeed of [42,7,2026]) for(const withProducers of [false,true]) {
  const api=await fresh({...water,...simulationStartSettings(),seed:randomSeed});
  const hunter=compile(api,source),photo=compile(api,named('Phototroph'));
  if(withProducers) seed(api,photo,48);
  seed(api,hunter,1);
  assert.equal(snapshot(api).food.length,0);
  const family=id=>{while(id>0 && id!==hunter && id!==photo) id=api.genome_parent(id);return id;};
  let state,firstDivision=null,seenTissue=false;
  for(let seconds=2;seconds<=600;seconds+=2) {
    state=advance(api,60);
    const hunters=state.cells.filter(c=>family(c.genome)===hunter);
    if(hunters.some(c=>c.generation>0)) firstDivision??=seconds;
    seenTissue ||= hunters.some(c=>c.gutEnergyByKind[1]>.01);
    if(!hunters.length) break;
  }
  const living=state.cells.filter(c=>family(c.genome)===hunter).length;
  rows.push({label,seed:randomSeed,producerFounders:withProducers?48:0,seconds:state.seconds,living,firstDivision,kills:state.predations,seenTissue});
  if(!withProducers) assert.ok(!living && state.births===0 && state.eaten===0 && state.photosynthesis===0,'Unfed controls exhaust their own stores');
  if(withProducers && label==='revised') assert.ok(living && firstDivision && seenTissue && state.predations>0 && state.photosynthesis>0,
    'The producer-supported hunter lineage must actually feed, divide and survive for ten minutes: '+JSON.stringify(rows.at(-1)));
}
console.table(rows);
