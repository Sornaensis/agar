// Isolated controls for mechanism and historical ecology comparisons.
export const scenarioFixtures = [
  {
    id: 'builtin:simulation:hunting-ground', name: 'Hunting ground',
    description: 'Watch predators grow their own sensors and piercers, approach living producers and feed on tissue.',
    environmentId: 'builtin:environment:grazing-shallows', seed: 42,
    populations: [
      { genomeId: 'builtin:genome:phototroph', count: 16 },
      { genomeId: 'builtin:genome:predator', count: 3 },
    ],
  },
  {
    id: 'builtin:simulation:mixed-ecosystem', name: 'Sunlit food web',
    description: 'Phototrophs feed grazers through released particles and contact. Predators hunt among them. No organic food is supplied.',
    environmentId: 'builtin:environment:grazing-shallows', seed: 42,
    populations: [
      { genomeId: 'builtin:genome:phototroph', count: 48 },
      { genomeId: 'builtin:genome:forager', count: 12 },
      { genomeId: 'builtin:genome:predator', count: 1 },
    ],
  },
  {
    id: 'builtin:simulation:light-and-shade', name: 'Light and shade',
    description: 'Mixotrophs bask near the lamp and feed in the shade. Rememberers follow chemical food cues.',
    environmentId: 'builtin:environment:light-and-shade', seed: 42,
    populations: [{ genomeId: 'builtin:genome:phototroph', count: 32 }, { genomeId: 'builtin:genome:mixotroph', count: 12 }, { genomeId: 'builtin:genome:rememberer', count: 2 }],
  },
  {
    id: 'builtin:simulation:nurse-rosette', name: 'Nurse rosette',
    description: 'Follow one founder through division. Inspect the attached adult and juvenile to see harvesting, growth and resource exchange.',
    environmentId: 'builtin:environment:colony-pool', seed: 42,
    populations: [{ genomeId: 'builtin:genome:nurse-rosette', count: 1 }],
  },
  {
    id: 'builtin:simulation:crown-rosette', name: 'Crown rosette',
    description: 'One founder grows a colony. A cell with two occupied ports can signal an edge cell to build another light harvester.',
    environmentId: 'builtin:environment:colony-pool', seed: 42,
    populations: [{ genomeId: 'builtin:genome:crown-rosette', count: 1 }],
  },
  {
    id: 'builtin:simulation:pulse-swimmers', name: 'Pulse swimmers',
    description: 'After division, mature cells start a pulse. Attached young cells relay it, swim briefly and rest before responding again.',
    environmentId: 'builtin:environment:colony-pool', seed: 42,
    populations: [{ genomeId: 'builtin:genome:pulse-swimmer', count: 3 }],
  },
];
