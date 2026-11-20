import { body, evaluation, memory } from './organelle-harness.mjs';

// Check topology and enzyme activity in the same snapshot. Identities connect
// observations in the study; the editable cell program reads each named physical port.
export function crownObservation() {
  return { maximumAttachedNeighbors:0, maximumOccupiedPorts:0, peakActiveRimHarvest:0, firstSpecialization:null, firstRimSecretion:null };
}

export function sampleCrown(api, state, observation) {
  if (state.population < 3) return;
  const centers = [], rims = [];
  for (const cell of state.cells) {
    const inventory = body(api,cell.id);
    observation.maximumAttachedNeighbors = Math.max(observation.maximumAttachedNeighbors,inventory.colony.neighbors);
    const trace = evaluation(api,cell.id);
    const read = name => trace.bindings.find(binding => binding.name === name)?.value;
    const signal = name => trace.signals.find(item => item.name === name)?.value;
    const ports = ['LeftBus', 'RightBus'];
    const occupied = ports.filter(name => signal('sensed.' + name + '.valid') === true);
    const count = occupied.length;
    const beacon = occupied.some(name => signal('sensed.' + name + '.signal_valid') === true &&
      signal('sensed.' + name + '.value') > 0.5);
    observation.maximumOccupiedPorts = Math.max(observation.maximumOccupiedPorts,count ?? 0);
    const harvest = inventory.cellular.assemblies.filter(a => a.name === 'RimSolar' && a.complete)
      .reduce((sum,a) => sum + a.activity * a.functionalMass,0);
    observation.peakActiveRimHarvest = Math.max(observation.peakActiveRimHarvest,harvest);
    const outlet = inventory.cellular.assemblies.some(a => a.name === 'RimOutlet' && a.complete && a.activity > 0);
    if (!observation.firstRimSecretion && read('rim') && count === 1 && beacon && outlet
        && cell.exudationRate > 0 && state.food.some(p => p.kind === 0 && p.energy > 0)) {
      observation.firstRimSecretion = {second:state.seconds, cell:cell.id, exportedEnergyPerSecond:cell.exudationRate};
    }
    if (read('center') && count >= 2 && inventory.colony.neighbors >= 2
        && memory(api,cell.id).some(slot => slot.name === 'rf021' && slot.value === 1)) centers.push({cell,count});
    if (read('rim') && count === 1 && beacon && inventory.colony.neighbors === 1
        && harvest > 0 && cell.carbonFixationRate > 0) rims.push({cell,harvest,peer:inventory.colony.neighborIds[0]});
  }
  if (observation.firstSpecialization) return;
  for (const center of centers) for (const rim of rims) {
    const a=center.cell, b=rim.cell;
    if (rim.peer !== a.id) continue;
    observation.firstSpecialization = { second:state.seconds, population:state.population, center:a.id,
      occupiedCenterPorts:center.count, rim:b.id, rimGeneration:b.generation, rimHarvesterMass:rim.harvest };
    return;
  }
}
