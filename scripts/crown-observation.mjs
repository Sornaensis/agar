import { body, evaluation, memory } from './organelle-harness.mjs';

// Check topology and enzyme activity in the same snapshot. Identities connect
// observations in the study; the editable cell program only sees count and mean.
export function crownObservation() {
  return { maximumAttachedNeighbors:0, maximumSignalPartners:0, peakActiveRimHarvest:0, firstSpecialization:null };
}

export function sampleCrown(api, state, config, observation) {
  if (state.population < 3) return;
  const centers = [], rims = [];
  for (const cell of state.cells) {
    const inventory = body(api,cell.id);
    observation.maximumAttachedNeighbors = Math.max(observation.maximumAttachedNeighbors,inventory.colony.neighbors);
    const trace = evaluation(api,cell.id);
    const read = field => trace.signals.find(signal => signal.name === 'sensed.Bus.' + field)?.value;
    const count = read('count');
    observation.maximumSignalPartners = Math.max(observation.maximumSignalPartners,count ?? 0);
    const harvest = inventory.cellular.assemblies.filter(a => a.name === 'RimSolar' && a.complete)
      .reduce((sum,a) => sum + a.activity * a.functionalMass,0);
    observation.peakActiveRimHarvest = Math.max(observation.peakActiveRimHarvest,harvest);
    if (read('valid') && count >= 2 && inventory.colony.neighbors >= 2
        && memory(api,cell.id).some(slot => slot.name === 'rf021' && slot.value === 1)) centers.push({cell,count});
    if (read('valid') && count === 1 && read('value') > 0.5 && inventory.colony.neighbors === 1
        && harvest > 0 && cell.carbonFixationRate > 0) rims.push({cell,harvest});
  }
  if (observation.firstSpecialization) return;
  const separation = (a,b,size) => Math.min(Math.abs(a-b),size-Math.abs(a-b));
  for (const center of centers) for (const rim of rims) {
    const a=center.cell, b=rim.cell;
    const distance = Math.hypot(separation(a.x,b.x,config.width),separation(a.y,b.y,config.height));
    if (distance > a.radius + b.radius + 1) continue;
    observation.firstSpecialization = { second:state.seconds, population:state.population, center:a.id,
      centerPartners:center.count, rim:b.id, rimGeneration:b.generation, rimHarvesterMass:rim.harvest };
    return;
  }
}
