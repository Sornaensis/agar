import { PRESETS } from './presets';
import type { EnvironmentDocument, GenomeDocument } from './storage';

function freeze<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}

/** Built-ins are recipes. Saving an edit creates an independently named user copy. */
export const BUILTIN_GENOMES: readonly GenomeDocument[] = freeze(PRESETS.map(preset => ({
  id: `builtin:genome:${preset.name.toLowerCase()}`, name: preset.name, source: preset.source,
  description: preset.description, createdAt: 0, updatedAt: 0,
})));

export const BUILTIN_ENVIRONMENTS: readonly EnvironmentDocument[] = freeze([
  {
    id: 'builtin:environment:still-water', name: 'Still water', description: 'A calm habitat with a steady food supply.', createdAt: 0, updatedAt: 0,
    config: { foodRate: 6, ambientTemperature: 0.5, ambientLight: 0, currentStrength: 0, initialNutrients: 0.3, sources: [] },
  },
  {
    id: 'builtin:environment:sunlit-pond', name: 'Sunlit pond', description: 'Light, a warm patch, a gentle vortex, and a nutrient spring.', createdAt: 0, updatedAt: 0,
    config: {
      foodRate: 6, ambientTemperature: 0.5, ambientLight: 0.12, currentStrength: 3, initialNutrients: 0.3,
      sources: [
        { kind: 0, x: 624, y: 364.8, strength: 0.95, radius: 280 },
        { kind: 1, x: 936, y: 243.2, strength: 0.35, radius: 180 },
        { kind: 2, x: 240, y: 478.8, strength: 0.25, radius: 260 },
        { kind: 3, x: 576, y: 418, strength: 0.7, radius: 220 },
      ],
    },
  },
  {
    id: 'builtin:environment:thermal-currents', name: 'Thermal currents', description: 'Warm and cool patches joined by moving, nutrient-rich water.', createdAt: 0, updatedAt: 0,
    config: {
      foodRate: 4, ambientTemperature: 0.5, ambientLight: 0.2, currentStrength: 8, initialNutrients: 0.5,
      sources: [
        { kind: 1, x: 300, y: 240, strength: 0.65, radius: 220 },
        { kind: 1, x: 900, y: 520, strength: -0.65, radius: 220 },
        { kind: 2, x: 600, y: 380, strength: 0.5, radius: 360 },
        { kind: 3, x: 440, y: 500, strength: 0.6, radius: 200 },
      ],
    },
  },
]);

export interface BuiltinSimulation {
  id: string; name: string; description: string; environmentId: string; seed: number;
  populations: Array<{ genomeId: string; count: number }>;
}
export const BUILTIN_SIMULATIONS: readonly BuiltinSimulation[] = freeze([
  {
    id: 'builtin:simulation:foragers', name: 'Foragers', description: 'Food-seeking cells in still water.',
    environmentId: 'builtin:environment:still-water', seed: 42,
    populations: [{ genomeId: 'builtin:genome:forager', count: 22 }],
  },
  {
    id: 'builtin:simulation:sunlit-pond', name: 'Sunlit pond', description: 'Photosynthetic cells following light and nutrients.',
    environmentId: 'builtin:environment:sunlit-pond', seed: 42,
    populations: [{ genomeId: 'builtin:genome:phototroph', count: 22 }],
  },
  {
    id: 'builtin:simulation:mixed-ecosystem', name: 'Mixed ecosystem', description: 'Foragers, phototrophs, and predators sharing one pond.',
    environmentId: 'builtin:environment:sunlit-pond', seed: 42,
    populations: [
      { genomeId: 'builtin:genome:forager', count: 14 },
      { genomeId: 'builtin:genome:phototroph', count: 14 },
      { genomeId: 'builtin:genome:predator', count: 4 },
    ],
  },
]);

export function isBuiltin(id: string): boolean { return id.startsWith('builtin:'); }
