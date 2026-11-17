import type { OrganelleKind } from './engine';

/** Display names mirror the engine's physical families; the Rust body is authoritative. */
export const ORGANELLE_CATALOG: ReadonlyArray<Readonly<{ kind: OrganelleKind; name: string; minimumMass: number; description: string; color: string }>> = Object.freeze([
  { kind: 'core', name: 'Core', minimumMass: 0.04, description: 'Translates family-specific assembly proteins and assembles organs using shared finite throughput. Each daughter needs a functional core.', color: '#d9c3f2' },
  { kind: 'photosynthetic', name: 'Photosynthetic unit', minimumMass: 0.04, description: 'Converts shared light into energy. Requires internal room and exposed membrane.', color: '#83d49b' },
  { kind: 'metabolic', name: 'Metabolic complex', minimumMass: 0.03, description: 'Raises energy-processing throughput; it needs an external energy source.', color: '#d9bc7c' },
  { kind: 'digestive', name: 'Digestive vacuole', minimumMass: 0.04, description: 'Stores exudate, tissue and waste separately. Processing is shared: tissue produces waste, and refractory waste is slower and yields less energy.', color: '#e8b875' },
  { kind: 'reserve', name: 'Reserve vacuole', minimumMass: 0.04, description: 'Adds energy and mineral storage. Its compartment occupies internal space.', color: '#a3b9c6' },
  { kind: 'transporter', name: 'Mineral transporter', minimumMass: 0.015, description: 'Absorbs dissolved minerals through the membrane at a paid, supply-limited rate.', color: '#7bd5b5' },
  { kind: 'phagocyte', name: 'Phagocytic cup', minimumMass: 0.025, description: 'Captures contacting particles and manageable objects into digestive space.', color: '#d7aa72' },
  { kind: 'flagellum', name: 'Flagellum', minimumMass: 0.025, description: 'Produces thrust and steering. Its anchor uses membrane space and its filament adds drag.', color: '#8dcbd9' },
  { kind: 'piercer', name: 'Piercing apparatus', minimumMass: 0.025, description: 'Damages contacting surfaces and can transfer material into available digestive space.', color: '#e8a295' },
  { kind: 'cortex', name: 'Protective cortex', minimumMass: 0.02, description: 'Resists damage and environmental stress, at a cost in material and membrane access.', color: '#a4afd8' },
  { kind: 'repair', name: 'Repair complex', minimumMass: 0.025, description: 'Increases paid repair throughput above the slow core baseline.', color: '#d4b5d7' },
  { kind: 'photoreceptor', name: 'Photoreceptor', minimumMass: 0.01, description: 'Measures local light and the direction of increasing brightness. Independent of photosynthesis.', color: '#e2ce83' },
  { kind: 'thermoreceptor', name: 'Thermoreceptor', minimumMass: 0.01, description: 'Measures local temperature and its gradient.', color: '#e9a08c' },
  { kind: 'chemoreceptor', name: 'Chemoreceptor', minimumMass: 0.015, description: 'Measures local mineral and organic-cue concentrations and their gradients.', color: '#8bd7b2' },
  { kind: 'mechanoreceptor', name: 'Mechanoreceptor', minimumMass: 0.01, description: 'Measures local flow and physical contact pressure and resistance.', color: '#9cbde7' },
  { kind: 'optical', name: 'Optical detector', minimumMass: 0.02, description: 'Detects visible objects by appearance, offset and motion. It cannot read hidden energy, genome or diet.', color: '#a9d8df' },
].map(item => Object.freeze(item)) as Array<Readonly<{ kind: OrganelleKind; name: string; minimumMass: number; description: string; color: string }>>);
