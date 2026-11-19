import { PRESETS } from './presets';
import type { EnvironmentConfig, EnvironmentDocument, GenomeDocument } from './storage';

function freeze<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}

/** Built-ins stay immutable. Saving an edit creates an independently named user copy. */
export const BUILTIN_GENOMES: readonly GenomeDocument[] = freeze(PRESETS.map(preset => ({
  id: `builtin:genome:${preset.name.toLowerCase().replaceAll(' ', '-')}`, name: preset.name, source: preset.source,
  description: preset.description, languageVersion: 3, createdAt: 0, updatedAt: 0,
})));

export const BUILTIN_ENVIRONMENTS: readonly EnvironmentDocument[] = freeze([
  {
    id: 'builtin:environment:still-water', name: 'Still water', description: 'A quiet, evenly lit pool where phototrophs turn dissolved carbon into biomass and exudate.', createdAt: 0, updatedAt: 0,
    config: { width: 640, height: 480, ambientTemperature: 0.5, ambientLight: 0.85, currentStrength: 0, initialNutrients: 0.3, sources: [] },
  },
  {
    id: 'builtin:environment:sunlit-pond', name: 'Sunlit pond', description: 'Light and minerals support producers; organic food comes from living cells and their remains.', createdAt: 0, updatedAt: 0,
    config: {
      ambientTemperature: 0.5, ambientLight: 0.4, currentStrength: 3, initialNutrients: 0.3,
      sources: [
        { kind: 0, x: 624, y: 364.8, strength: 0.95, radius: 280 },
        { kind: 1, x: 936, y: 243.2, strength: 0.35, radius: 180 },
        { kind: 2, x: 240, y: 478.8, strength: 0.25, radius: 260 },
        { kind: 3, x: 576, y: 418, strength: 0.7, radius: 220 },
      ],
    },
  },
  {
    id: 'builtin:environment:thermal-currents', name: 'Thermal currents', description: 'A producer-powered habitat with warm and cool currents and a mineral spring.', createdAt: 0, updatedAt: 0,
    config: {
      ambientTemperature: 0.5, ambientLight: 0.45, currentStrength: 8, initialNutrients: 0.5,
      sources: [
        { kind: 0, x: 600, y: 380, strength: 0.35, radius: 320 },
        { kind: 1, x: 300, y: 240, strength: 0.65, radius: 220 },
        { kind: 1, x: 900, y: 520, strength: -0.65, radius: 220 },
        { kind: 2, x: 600, y: 380, strength: 0.5, radius: 360 },
        { kind: 3, x: 440, y: 500, strength: 0.6, radius: 200 },
      ],
    },
  },
  {
    id: 'builtin:environment:grazing-shallows', name: 'Grazing shallows',
    description: 'A cool, sunlit pond with a mild current. Cells release organic food into the water and exchange resources on contact.',
    createdAt: 0, updatedAt: 0,
    config: { width: 400, height: 400, ambientTemperature: 0.15, ambientLight: 0.95,
      currentStrength: 1, initialNutrients: 0.9, exchangeRate: 0.2, sources: [] },
  },
  {
    id: 'builtin:environment:light-and-shade', name: 'Light and shade',
    description: 'A lamp lights the producer bed. A current carries cells and released organic matter between the bright center and the shaded margins.',
    createdAt: 0, updatedAt: 0,
    config: { width: 400, height: 400, ambientTemperature: 0.4, ambientLight: 0.24,
      currentStrength: 4, initialNutrients: 1, exchangeRate: 0.15,
      sources: [{ kind: 0, x: 125, y: 200, strength: 0.95, radius: 95 }] },
  },
  {
    id: 'builtin:environment:colony-pool', name: 'Colony pool', description: 'Still, evenly lit water with dissolved minerals for observing division and contact between relatives.', createdAt: 0, updatedAt: 0,
    config: { width:400, height:400, ambientTemperature:0.5, ambientLight:0.85, currentStrength:0, initialNutrients:3, sources:[] },
  },
  {
    id: 'builtin:environment:current-bands', name: 'Current bands',
    description: 'A larger habitat with opposing currents, a sheltered channel, a drifting barrier, and local nutrient input and removal.', createdAt: 0, updatedAt: 0,
    config: {
      width: 2400, height: 1520, ambientTemperature: 0.5, ambientLight: 0, currentStrength: 0, initialNutrients: 0.3,
      sources: [
        { kind: 0, x: 750, y: 440, strength: 0.95, radius: 620 },
        { kind: 0, x: 1750, y: 1050, strength: 0.85, radius: 560 },
        { kind: 6, x: 1200, y: 430, strength: 0.6, radius: 180, angle: 0, length: 2400 },
        { kind: 6, x: 1200, y: 1070, strength: -0.6, radius: 180, angle: 0, length: 2400 },
        { kind: 2, x: 1920, y: 760, strength: 0.45, radius: 360 },
        { kind: 3, x: 390, y: 430, strength: 0.55, radius: 140 },
        { kind: 4, x: 2030, y: 1070, strength: 0.45, radius: 160 },
      ],
      barriers: [
        { x: 1000, y: 680, length: 760, width: 40, angle: 0, floating: false },
        { x: 1300, y: 840, length: 760, width: 40, angle: 0, floating: false },
        { x: 850, y: 1040, length: 170, width: 36, angle: 0.35, floating: true },
      ],
    },
  },
].map(item => ({ ...item, config: {
  width: item.config.width ?? 1200, height: item.config.height ?? 760,
  ambientTemperature: item.config.ambientTemperature, ambientLight: item.config.ambientLight,
  currentStrength: item.config.currentStrength, initialNutrients: item.config.initialNutrients,
  barriers: item.config.barriers ?? [],
  gases: { initialOxygen: 1, initialCarbon: 1, mode: 'open', exchangeRate: item.config.exchangeRate ?? 0.02,
    oxygenEquilibrium: 1, carbonEquilibrium: 1 },
  sources: item.config.sources.map(source => ({ angle: 0, length: 600, ...source })),
} as EnvironmentConfig })));

export interface BuiltinSimulation {
  id: string; name: string; description: string; environmentId: string; seed: number;
  populations: Array<{ genomeId: string; count: number }>;
}
/** The standard mutation rate applies to every built-in launch. */
export function simulationStartSettings(): { mutationRate: number } {
  return { mutationRate: 0.1 };
}

export const BUILTIN_SIMULATIONS: readonly BuiltinSimulation[] = freeze([
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
    description: 'One founder grows a colony. Around 160 simulated seconds, edge cells beside a signaling center build extra light harvesters.',
    environmentId: 'builtin:environment:colony-pool', seed: 42,
    populations: [{ genomeId: 'builtin:genome:crown-rosette', count: 1 }],
  },
  {
    id: 'builtin:simulation:pulse-swimmers', name: 'Pulse swimmers',
    description: 'After division, mature cells start a pulse. Attached young cells relay it, swim briefly and rest before responding again.',
    environmentId: 'builtin:environment:colony-pool', seed: 42,
    populations: [{ genomeId: 'builtin:genome:pulse-swimmer', count: 3 }],
  },
]);

export function isBuiltin(id: string): boolean { return id.startsWith('builtin:'); }
