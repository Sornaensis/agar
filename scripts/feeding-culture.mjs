// Controlled assays use ordinary cells and the exported simulation operations.
// A finite donor's death transfers its existing carbon and minerals into tissue.
import assert from 'node:assert/strict';
import {advance, body, compile, error, seed, snapshot} from './organelle-harness.mjs';

const inert = 'thrust=0;turn=0;divide=false;absorb=0;feed=0;';
export function lyseDonor(api,x,y) {
  const donor = api.place_cell(compile(api,inert),x,y);
  assert.ok(donor,error(api));
  assert.equal(api.set_cell_stores(donor,1,0,0),1,error(api));
}

// A finite patch of detritus generated entirely by biological processes. Grow
// the shipped producers in sunlight, then let a single heat pulse kill them.
// Return the location of a real packet for the subsequent consumer introduction.
export function prepareBloom(api, donorSource, scattered = false) {
  const donor = compile(api,donorSource);
  const count=scattered ? 100 : 25;
  if(scattered) seed(api,donor,count);
  else seed(api,donor,count,200,200);
  let state = advance(api,60*30);
  assert.equal(state.population,count);
  assert.ok(state.photosynthesis > 0);
  api.set_environment(1,.85,0);
  while (state.population && state.seconds < 150) state=advance(api,30);
  assert.equal(state.population,0,'All donors die from the physical heat exposure');
  assert.ok(state.food.some(p=>p.kind===1) && state.deaths===count);
  api.set_environment(.15,.85,0);
  if(!scattered) return state.food.reduce((a,b)=>a.energy>b.energy ? a : b);
  // Start the older, obligate feeder in a rich patch of this finite bloom.
  // Rank once by the actual energy within one local capture neighborhood.
  let best=null,highest=-1;
  for(const food of state.food) {
    let energy=0;
    for(const neighbor of state.food) {
      if(Math.hypot(neighbor.x-food.x,neighbor.y-food.y)<12) energy+=neighbor.energy;
    }
    if(energy>=highest) { highest=energy; best=food; }
  }
  return best;
}

export function placeByTissue(api,cell,noseDistance) {
  // First produce a real finite carcass out of contact. Place the consumer
  // relative to the leading real packet, so capture still requires approach.
  lyseDonor(api,70,70);
  const particles = snapshot(api).food.filter(p => p.kind === 1);
  assert.ok(particles.length);
  const intake = body(api,cell.id).cellular?.assemblies.find(a => a.kind === 'import(particle)' && a.complete);
  const angle = cell.angle + (intake?.at ?? 0);
  const dx = Math.cos(angle), dy = Math.sin(angle);
  const front = particles.reduce((a,b) => a.x*dx + a.y*dy < b.x*dx + b.y*dy ? a : b);
  const distance = cell.radius + noseDistance;
  assert.ok(api.move_cell(cell.id,front.x - dx*distance,front.y - dy*distance,0),error(api));
  return front;
}
