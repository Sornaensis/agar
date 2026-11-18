# Gas exchange and material cycles

This is the proposed next biology change, not behavior implemented by the environment editor update.

## What the simulator currently represents

`Environment` owns one conserved dissolved mineral field, temperature, instantaneous light, prescribed currents, and a nonconserved organic chemical cue. Organic particles separately hold energy and minerals, with distinct exudate, tissue, and waste processing. `World::photosynthesize_in` presently converts available light directly into usable energy and organic exudate; it does not consume inorganic carbon. `digest_in` supplies energy and minerals without requiring oxygen. Cellular products and structures consume a single material stock.

Adding oxygen and carbon dioxide meters to those pathways would not create a carbon cycle. Photosynthesis must fix actual carbon, and respiration must transfer that carbon back to dissolved inorganic carbon. Mineral nutrients still have their own role. This distinction is central to conservation-based phytoplankton models [3].

## Smallest useful chemical model

Start with dissolved oxygen and a dissolved inorganic carbon pool alongside the current mineral field. Introduce organic carbon in cell reserves, physical products, bodies, gut contents, and particles. Keep usable energy separate from matter. The existing mineral field can remain an explicitly simplified noncarbon growth resource until nitrogen and phosphorus are represented separately.

| Store | Role |
| --- | --- |
| Dissolved O2 | Oxidant available for aerobic respiration; produced by oxygenic photosynthesis |
| Dissolved inorganic carbon | Carbon source for fixation and destination for respired carbon |
| Organic carbon | Conserved carbon in reserves, bodies, products, gut contents, exudate, tissue, and waste |
| Mineral nutrients | Noncarbon resources consumed in construction and returned by decomposition |
| Usable energy | Paid metabolic work; neither carbon nor mineral mass |

Use internally consistent amount units and give every patch a defined water volume. Display concentrations as amount divided by volume. A normalized abstract simulation can use arbitrary consistent units; physical oxygen saturation values require a temperature scale, depth, and pressure assumptions. The current normalized temperature control alone is insufficient for calibrated mg/L chemistry.

Initially treat dissolved inorganic carbon as one effective available pool. Describe it honestly as an approximation. Real inorganic carbon includes dissolved CO2, bicarbonate, and carbonate; pH-dependent availability and buffering can follow after the simple carbon budget works. Do not silently label a total-inorganic-carbon pool as free CO2.

## Reactions belong to mechanisms

Use typed reaction definitions associated with actual built machinery. The genome regulates the machinery through the existing products, recipes, and activity controls. No genome-name lookup or global phototroph/heterotroph flag should determine gas behavior.

For a simplified carbohydrate-like organic carbon unit, the reactions are:

- Oxygenic photosynthesis: CO2 + H2O + light → CH2O + O2.
- Aerobic respiration: CH2O + O2 → CO2 + H2O + usable energy + dissipated heat.

Water is an explicit abundant reservoir in this first approximation; the formulas describe gross balanced chemistry, not all intermediate reactions. Carbon fixation is limited by local light, carbon supply, built reaction capacity, and reserve capacity. Respiration is limited by actual available organic carbon, oxygen, and respiratory machinery. Every reaction spends its complete set of substrates and produces only the matching outputs.

Phototrophs also respire. They can produce net oxygen in adequate light and consume net oxygen in darkness. The direction and size of their net gas flux must emerge from their simultaneous reaction rates. Aquatic oxygen concentrations reflect production, respiration, transport, and atmospheric exchange [1, 2].

Keep digestion separate from respiration. Digestion converts captured material into accessible substrates; respiration releases usable energy from those substrates. Existing `digest(tissue)`, `digest(exudate)`, and `digest(waste)` machinery can retain substrate-specific processing while a separately assembled respiratory mechanism determines aerobic energy yield. Food must not grant the same energy once during digestion and again during respiration.

Do not let particles disappear through a hidden unlimited oxygen-consuming metabolism. Prefer explicit decomposers for biological recycling. If a background decay reaction is retained, expose and account for its substrates, products, rate, and gas demand. Abiotic fragmentation can change particle size or accessibility without destroying carbon.

## Exchange and transport

Use the current dimension-aware field grid and barrier topology. Store channels in separate contiguous arrays, share face-flow calculations, and reuse scratch buffers. Give each species its own diffusion coefficient and conservative fluxes. Avoid introducing per-organism whole-world scans.

Gas exchange across a cell membrane is a concentration-gradient flux with finite permeability and surface area. It should not require a newly invented pump for ordinary passive O2/CO2 diffusion. Optional transport machinery can alter permeability or support active carbon uptake. Internal gas pools make this boundary explicit; aggregate patch-level allocations ensure that overlapping cells cannot each claim the same external oxygen or carbon.

Offer two environment boundary conditions:

- **Sealed:** no atmospheric exchange. Gas and carbon budgets close apart from explicit sources and absorbers.
- **Open:** exchange at designated exposed-water regions with a specified reservoir. Compute a signed flux proportional to exchange coefficient × exposed area × (equilibrium concentration − water concentration). Record both inflow and outflow. Do not reset every patch to a target concentration each tick.

Air–water exchange can add or remove oxygen depending on saturation, and temperature affects equilibrium oxygen concentration [1, 2]. A flow generator moves dissolved material; it does not automatically create oxygen. A separate aerator is an explicit exchange object. In this 2D model, barriers are full-depth transport obstacles; depth-dependent flow around or beneath floating objects would require another physical model.

## Allocation and correctness

Build on the existing shared resource allocation: collect requests, determine each reaction's feasible extent from every required substrate, and commit matching withdrawals and products. Competing cells share the same finite field inventories. A single limiting substrate must scale the whole reaction rather than independently clamping products after consumption. Use a documented phase order for exchange, reactions, and transport so results do not depend on actor iteration order.

The essential checks are:

1. A sealed world conserves carbon across dissolved, living, and particulate stores, including growth, damage, death, digestion, exudation, and division.
2. No reaction or transport step makes a substrate negative or creates energy without spending its specified inputs.
3. Oxygen starvation limits aerobic energy generation even with abundant food; carbon starvation limits fixation despite abundant light.
4. Phototroph respiration continues without light; successful photosynthesis produces oxygen only in proportion to actual fixation.
5. Sources, absorbers, and atmospheric fluxes reconcile against external species ledgers.
6. Barriers block the appropriate transport faces, while world resize and checkpoint replay preserve chemical amounts and future deterministic behavior.

The first implementation should deliver this closed carbon/oxygen cycle. Nitrogen and phosphorus quotas, fermentation, nitrate respiration, carbonate chemistry, and temperature calibration can then be added as explicit reactions and species without redefining the underlying accounting.

## Sources

1. Senn, Downing-Kunz, and Novick, *Dissolved oxygen: Chapter 6*, U.S. Geological Survey (2016). https://www.usgs.gov/publications/dissolved-oxygen-chapter-6
2. U.S. Geological Survey, *Dissolved Oxygen and Water*. https://www.usgs.gov/water-science-school/science/dissolved-oxygen-and-water
3. Inomura et al., *A Mechanistic Model of Macromolecular Allocation, Elemental Stoichiometry, and Growth Rate in Phytoplankton*, Frontiers in Microbiology (2020). https://www.frontiersin.org/journals/microbiology/articles/10.3389/fmicb.2020.00086/full
