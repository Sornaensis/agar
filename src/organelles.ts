import type { OrganelleKind, MechanismKind, ProductRole } from './engine';

/** Display names mirror the engine's physical families; the Rust body is authoritative. */
export const ORGANELLE_CATALOG: ReadonlyArray<Readonly<{ kind: OrganelleKind; name: string; minimumMass: number; description: string; color: string }>> = Object.freeze([
  { kind: 'core', name: 'Core', minimumMass: 0.04, description: 'Translates family-specific assembly proteins and assembles organs using shared finite throughput. Each daughter needs a functional core.', color: '#d9c3f2' },
  { kind: 'photosynthetic', name: 'Photosynthetic unit', minimumMass: 0.04, description: 'Uses shared light to fix inorganic carbon and release oxygen. Requires internal room and exposed membrane.', color: '#83d49b' },
  { kind: 'metabolic', name: 'Metabolic complex', minimumMass: 0.03, description: 'Raises aerobic respiratory capacity. Organic carbon and oxygen are required to release usable energy.', color: '#d9bc7c' },
  { kind: 'digestive', name: 'Digestive vacuole', minimumMass: 0.04, description: 'Stores exudate, tissue and waste separately. Digestion extracts organic carbon and minerals; respiration supplies usable energy.', color: '#e8b875' },
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


export interface MechanismDefinition {
  kind: MechanismKind; name: string; constructor: string; target: string | null;
  roles: readonly ProductRole[]; description: string; color: string; passive?: boolean;
}

/** One operation per machine. The compiler/runtime define physical rates and role checks. */
export const MECHANISM_CATALOG: readonly MechanismDefinition[] = Object.freeze([
  { kind:'support()', name:'Structural support', constructor:'support', target:null, roles:['scaffold'], description:'Supports viable envelope area. Provides structural support only.', color:'#bdc2b0', passive:true },
  { kind:'translate()', name:'Translation machinery', constructor:'translate', target:null, roles:['scaffold','enzyme'], description:'Supplies the shared work used to synthesize products, install components and perform paid growth and repair.', color:'#dfbf7f' },
  { kind:'store(energy)', name:'Energy storage', constructor:'store', target:'energy', roles:['membrane'], description:'Adds usable-energy capacity. Storage remains present when activity is zero.', color:'#e3c483', passive:true },
  { kind:'store(mineral)', name:'Mineral storage', constructor:'store', target:'mineral', roles:['membrane'], description:'Adds capacity for free minerals. It does not import minerals.', color:'#9ebac7', passive:true },
  { kind:'store(food)', name:'Food compartment', constructor:'store', target:'food', roles:['membrane'], description:'Holds captured exudate, tissue and waste. Separate enzymes digest each substrate.', color:'#b9a1d9', passive:true },
  { kind:'harvest(light)', name:'Light harvesting', constructor:'harvest', target:'light', roles:['anchor','pigment','enzyme'], description:'Uses shared local light and inorganic carbon to produce organic carbon and oxygen, with excess carbon released as exudate. Does not provide a light sensor.', color:'#83d49b' },
  { kind:'import(mineral)', name:'Mineral import', constructor:'import', target:'mineral', roles:['anchor','transporter'], description:'Imports available dissolved minerals through the membrane using paid transport.', color:'#7bd5b5' },
  { kind:'import(particle)', name:'Particle capture', constructor:'import', target:'particle', roles:['anchor','motor'], description:'Captures contacting organic particles into available food storage. Does not engulf living cells.', color:'#d7aa72' },
  { kind:'engulf()', name:'Cell engulfment', constructor:'engulf', target:null, roles:['anchor','motor','membrane'], description:'Engulfs geometrically manageable contacting cells into available food storage.', color:'#d6a788' },
  { kind:'digest(exudate)', name:'Exudate digestion', constructor:'digest', target:'exudate', roles:['enzyme'], description:'Extracts organic carbon and minerals from stored exudate. Other substrates need their own enzymes; usable energy requires respiration.', color:'#e9c777' },
  { kind:'digest(tissue)', name:'Tissue digestion', constructor:'digest', target:'tissue', roles:['enzyme'], description:'Extracts organic carbon and minerals from stored tissue, releasing refractory waste.', color:'#e8a295' },
  { kind:'digest(waste)', name:'Waste digestion', constructor:'digest', target:'waste', roles:['enzyme'], description:'Processes stored refractory waste at its lower physical speed and yield.', color:'#b6a08b' },
  { kind:'propel()', name:'Propulsion', constructor:'propel', target:null, roles:['anchor','motor','filament'], description:'Applies force in its direction. Attachment position determines torque; opposing forces still cost energy.', color:'#8dcbd9' },
  { kind:'pierce()', name:'Contact piercing', constructor:'pierce', target:null, roles:['anchor','filament','enzyme'], description:'Damages a surface reached from its attachment along its declared direction. Released food requires separate capture or engulfment machinery.', color:'#e8a295' },
  { kind:'shield()', name:'Surface protection', constructor:'shield', target:null, roles:['scaffold'], description:'Protects the surface near its attachment from contact damage and contributes resistance to environmental stress.', color:'#a4afd8' },
  { kind:'repair()', name:'Repair machinery', constructor:'repair', target:null, roles:['enzyme'], description:'Performs paid repair of existing damage, using shared material and work.', color:'#d4b5d7' },
  { kind:'sense(light)', name:'Light sensing', constructor:'sense', target:'light', roles:['anchor','receptor','pigment'], description:'Measures light at its attachment point. Named receptor readings do not reveal a gradient or harvest energy.', color:'#e2ce83' },
  { kind:'sense(heat)', name:'Heat sensing', constructor:'sense', target:'heat', roles:['anchor','receptor'], description:'Measures temperature at its attachment point.', color:'#e9a08c' },
  { kind:'sense(chemical)', name:'Chemical sensing', constructor:'sense', target:'chemical', roles:['anchor','receptor'], description:'Measures mineral and organic-cue concentrations at its attachment point. Compare receptors or regulatory memory to navigate.', color:'#8bd7b2' },
  { kind:'sense(contact)', name:'Contact and flow sensing', constructor:'sense', target:'contact', roles:['anchor','receptor','filament'], description:'Measures local flow, contact pressure and surface resistance.', color:'#9cbde7' },
  { kind:'sense(optical)', name:'Optical sensing', constructor:'sense', target:'optical', roles:['anchor','receptor','pigment'], description:'Measures visible object appearance, offset and motion. Hidden identity and energy remain inaccessible.', color:'#a9d8df' },
  { kind:'grow()', name:'Envelope growth', constructor:'grow', target:null, roles:['membrane','enzyme'], description:'Performs paid envelope growth within structural support and space limits.', color:'#b9a1d9' },
  { kind:'replicate()', name:'Genome replication', constructor:'replicate', target:null, roles:['scaffold','replicase'], description:'Copies genetic material over time using paid material, energy and work.', color:'#b7aadf' },
  { kind:'constrict()', name:'Division constriction', constructor:'constrict', target:null, roles:['anchor','motor','constrictor'], description:'Advances paid constriction after replication and feasible division geometry.', color:'#dfa4a0' },
  { kind:'recycle()', name:'Material recycling', constructor:'recycle', target:null, roles:['enzyme'], description:'Supplies paid disassembly work. A recipe’s recycle expression selects machinery to dismantle; recovered ingredients keep their original identity.', color:'#8fbaa1' },
  { kind:'respire(oxygen)', name:'Aerobic respiration', constructor:'respire', target:'oxygen', roles:['enzyme','membrane'], description:'Consumes organic carbon and oxygen, releasing inorganic carbon and usable energy. Works in light or darkness; every reaction is limited by both substrates.', color:'#79b7df' },
  { kind:'adhere()', name:'Cell adhesion', constructor:'adhere', target:null, roles:['anchor','membrane'], description:'Holds contacting cells together while both use active adhesion machinery and accept each other’s published scalar. Set the matching register with adhere_message.', color:'#bfa4d3' },
  { kind:'junction(energy)', name:'Energy junction', constructor:'junction', target:'energy', roles:['membrane','transporter'], description:'Shares usable energy between attached cells with active energy junctions. Energy flows toward the less full reserve.', color:'#e6bd6d' },
  { kind:'junction(mineral)', name:'Mineral junction', constructor:'junction', target:'mineral', roles:['membrane','transporter'], description:'Shares free minerals between attached cells with active mineral junctions, toward the less full store.', color:'#90c6b6' },
  { kind:'junction(carbon)', name:'Organic carbon junction', constructor:'junction', target:'carbon', roles:['membrane','transporter'], description:'Shares stored organic carbon between attached cells with active carbon junctions, toward the less full store.', color:'#d09691' },
] as MechanismDefinition[]);
