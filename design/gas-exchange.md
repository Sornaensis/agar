# Gas exchange and material cycles

Implemented in engine ABI 12 / checkpoint 9. This document describes the current model, its conservation rules, and its deliberate approximations.

## Chemical stores and units

Minerals remain the noncarbon construction resource. Two separate dissolved fields hold oxygen (O₂) and effective dissolved inorganic carbon (DIC). DIC is an available inorganic-carbon pool, not a calibrated measurement of free CO₂. Each physical 20 × 20 patch contains one abstract water-volume unit; field amounts therefore have the same numerical value as patch concentrations.

| Store | Representation |
| --- | --- |
| Dissolved oxygen | Nonnegative amount in each water patch |
| Dissolved inorganic carbon | Nonnegative amount in each water patch |
| Internal gases | Finite O₂ and DIC amounts inside each organism |
| Organic carbon reserves | Explicit amount available for respiration or construction |
| Structural organic carbon | One carbon unit per unit of existing structural/product mass |
| Food and gut carbon | Chemical substrate potential divided by 20 energy units per carbon unit |
| Usable energy | ATP-like reserve used to pay for biological work |
| Mineral nutrients | Separate conserved noncarbon material |

Food's legacy `energy` field now means chemical substrate potential. It is not immediately usable energy. ATP-like usable energy never turns into carbon when a cell dies or is eaten. Material in products, assemblies, frames, replication machinery and gut contents is counted once.

## Reactions and physical machinery

The simplified balanced reactions are:

- Oxygenic fixation: inorganic C + light → organic C + O₂.
- Aerobic respiration: organic C + O₂ → inorganic C + usable energy.

Water is an abundant implicit reservoir. One carbon unit and one oxygen unit participate in each reaction extent. Fixation stores 20 chemical-potential energy units per carbon; respiration releases 16 usable units, with the remainder dissipated. These are model parameters, not biochemical calibration.

`harvest(light)` and legacy photosynthetic organs fix carbon using shared local light, available internal DIC, built capacity and reserve space. They produce only the oxygen corresponding to actual fixation. A fixed 12% of newly fixed carbon is released as exudate; the rest enters organic reserves.

`respire(oxygen)` requires both enzyme and membrane products. Built cores provide basal respiration toward a working reserve of 25% of usable-energy capacity; legacy metabolic complexes and explicitly constructed respiratory machinery can fill the larger store. Respiration is limited by organic carbon, internal oxygen, usable-energy storage and functional machinery. Activity regulates the explicit machine. Both phototrophs and heterotrophs respire; no genome name selects a metabolic behavior. A producer can therefore consume net oxygen in darkness.

Digestion transfers accessible carbon and minerals from captured food into reserves. It does not grant a second energy reward. Refractory residues retain their carbon. Construction consumes organic carbon and minerals in a fixed quota; recycling returns the structural carbon it releases. Division partitions existing organic and internal-gas pools. Death and predation transfer actual carbon and release internal gases.

The retained background decay process is explicitly aerobic: it consumes dissolved oxygen, releases the same amount of inorganic carbon and returns associated minerals. Its reaction total is exposed separately from organism respiration. It stops when oxygen is unavailable, retaining unprocessed particle carbon. This is a simplified background decomposer process, not a population of modeled microbes.

## Membrane exchange and transport

O₂ and DIC cross membranes passively according to the external/internal concentration difference, finite permeability and cell surface size. Internal gas volume scales with body area. All organisms request from the same pre-exchange field; outward transfers are collected, and competing inward requests share available stock proportionally. No organism can spend oxygen or carbon already assigned to another.

Water transport uses separate contiguous arrays for minerals, O₂ and DIC, with shared face velocities and reusable buffers. Diffusion and advection conserve each inventory. Full-depth barriers block the same faces as mineral transport. Resizing redistributes existing amounts without creating new matter. Currents move gases; they do not create them.

The tick order is explicit: environment transport and external exchange; background aerobic decay; observation and regulation; membrane exchange; respiration; movement and maintenance; digestion and mineral uptake; collisions and food capture; fixation; predation; construction and lifecycle transfers. Captured food is digested on a later tick, and newly digested or fixed carbon becomes available to respiration on the following tick. Competing requests within a phase share the same inventory.

## Boundaries and environment objects

The builder's **Gases & atmosphere** section sets initial O₂/DIC amounts and the boundary condition. Changing starting amounts affects the next reset only. Live controls change the boundary without replacing current gas fields.

- **Sealed:** no background atmospheric flux.
- **Open:** every uncovered water patch has finite signed exchange with the specified reservoir. Excess gas leaves and undersaturated water receives gas.
- **Oxygen emitter / absorber:** local, explicitly accounted O₂ input or removal.
- **Carbon emitter / absorber:** local, explicitly accounted inorganic-carbon input or removal.
- **Aerator:** a local reservoir connection, including in an otherwise sealed world. It can add or remove either gas.

Emitter/absorber strength ranges from 0 to 1, with total requested rate `1.5 × strength` amount units per second distributed over the object's footprint. Absorbers remove only available stock. A unit-strength aerator exchanges five abstract water volumes per second over its footprint. Background exchange rate ranges from 0 to 1 per second; initial and equilibrium concentrations range from 0 to 100.

Signed atmospheric exchange uses `1 − exp(−rate × dt)` of the concentration difference, avoiding overshoot or a hidden reset. Oxygen equilibrium has an explicitly abstract temperature factor `1.25 − 0.5 × temperature`; DIC equilibrium is fixed. This is not a Celsius-, pressure- or salinity-calibrated solubility model.

All built-in environments use open water with rate 0.02/s and equilibrium concentrations of 1. Blank recipes, imports without gas settings and pre-version-9 checkpoints default to sealed water with initial O₂/DIC of 1 each per patch. Copying a recipe preserves its gas settings. Local objects remain explicit external actions in either mode.

## Inspection and accounting

The player and builder provide **Oxygen** and **Inorganic carbon** overlays on fixed color scales. Organism inspection shows organic reserve, internal gases, fixation and respiration rates. Programs can read `carbon.organic`, `carbon.inorganic`, `oxygen`, `respiration.rate` and `carbon.fixation.rate`; the two reaction rates use carbon units per second.

Metrics expose all carbon stores, internal and dissolved oxygen, external inputs/removals, cumulative fixation, organism respiration, background decay and legacy imports. The budgets are:

```
carbon present + carbon removed = carbon introduced
oxygen present + oxygen removed + organism respiration + decay
    = oxygen introduced + carbon fixed
```

Carbon present includes both internal and external inorganic carbon, structural/reserve/gut carbon and particles. Introductions include initial water, added organisms and supplied organic food. Source/sink and atmospheric transfers enter external ledgers. The same equations hold in open and sealed environments.

Checkpoints 3–8 remain readable. Migration preserves existing physical structures, minerals, usable energy and sources, initializes gas fields explicitly, and classifies pre-existing organic inventory once in a legacy carbon-import ledger. It does not recreate cells from their genome. Continuation follows the new chemistry and is deterministic within the current engine; it is not intended to reproduce an older engine's biological trajectory. New checkpoints retain all chemical fields, internal pools, boundary settings and reaction ledgers.

## Limits and future chemistry

This implementation establishes a conserved oxygen/carbon cycle. Nitrogen and phosphorus remain combined in the mineral resource. Separate carbon reserves and gas pools do not change the legacy mechanical `body.mass` or cell radius. Carbonate speciation, pH, salinity, physical depth, anaerobic pathways and gases such as methane or nitrogen are not modeled. Metabolic heat dissipation is not a calibrated thermodynamic water model. Floating barriers remain full-depth transport obstacles rather than buoyant 3D objects.

## Biological references

The model choices are simplified implementations, informed by the underlying processes below rather than fitted to their quantitative data.

1. Senn, Downing-Kunz, and Novick, *Dissolved oxygen: Chapter 6*, U.S. Geological Survey (2016): https://www.usgs.gov/publications/dissolved-oxygen-chapter-6
2. U.S. Geological Survey, *Dissolved Oxygen and Water*: https://www.usgs.gov/water-science-school/science/dissolved-oxygen-and-water
3. Inomura et al., *A Mechanistic Model of Macromolecular Allocation, Elemental Stoichiometry, and Growth Rate in Phytoplankton*, Frontiers in Microbiology (2020): https://www.frontiersin.org/journals/microbiology/articles/10.3389/fmicb.2020.00086/full
