//! Contact adhesion is a reversible surface interaction. Register matching
//! selects partners; the smaller active adhesive surface limits their coupling.
use super::*;

pub(super) const CONTACT_GAP: f64 = 1.0;
pub(super) const RELEASE_GAP: f64 = 2.0;
pub(super) const MATURE: u8 = 15; // Recruited after consecutive ticks.
pub(super) const CLONAL: u8 = 16; // Retained at fission, possibly passed to a later daughter.

mod topology;
pub(super) use topology::DivisionBonds;
const GAP_RELAXATION: f64 = 4.0;
const SHEAR_DAMPING: f64 = 0.5;
// A site is a reusable quantity of installed, intact, exposed material.
const ADHESIN_PER_SITE: f64 = 0.0015;
const ACCELERATION_PER_SITE: f64 = 360.0;

// Maximum amounts crossing one unit of active junction mass per second at a
// full concentration difference. Carbon is measured in reserve units, ATP in energy units.
const JUNCTION_RATES: [f64; 3] = [120.0, 1.5, 6.0];

#[derive(Default, Clone, Copy)]
struct ContactPorts {
    sites: u32,
    junction: [f64; 3],
    // One-based handle into the sparse list of endpoints sharing a compatible port.
    account: usize,
}

impl ContactPorts {
    fn of(cell: &Cell) -> Self {
        use MechanismKind::*;
        let mut ports = Self::default();
        let mut exposed_adhesin = 0.0;
        for assembly in cell.body.cellular.iter().flat_map(|state| &state.assemblies) {
            let mass = match assembly.recipe.kind {
                AssemblyKind::Mechanism(Adhere) => {
                    if assembly.functional_mass() > 0.0 {
                        let material: f64 = assembly.ingredients.iter().zip(&assembly.recipe.parts)
                            .filter(|(_, p)| p.product.kind == crate::cellular::ProductKind::Adhesin)
                            .map(|(ingredient, _)| ingredient.mass).sum();
                        exposed_adhesin += material * assembly.integrity * assembly.activity;
                    }
                    continue;
                }
                AssemblyKind::Mechanism(JunctionEnergy) => &mut ports.junction[0],
                AssemblyKind::Mechanism(JunctionMineral) => &mut ports.junction[1],
                AssemblyKind::Mechanism(JunctionCarbon) => &mut ports.junction[2],
                _ => continue,
            };
            *mass += assembly.functional_mass() * assembly.activity;
        }
        ports.sites = (exposed_adhesin / ADHESIN_PER_SITE + 1e-12).floor() as u32;
        ports
    }
}

fn contact_geometry(env: &Environment, a: &Cell, b: &Cell, max_gap: f64)
    -> Option<(f64, f64, f64, f64)> {
    let dx = delta(b.x - a.x, env.width);
    let dy = delta(b.y - a.y, env.height);
    let distance = dx.hypot(dy);
    let gap = distance - a.radius() - b.radius();
    if distance <= 1e-9 || gap > max_gap || env.segment_blocked((a.x, a.y), (b.x, b.y)) { return None; }
    Some((dx, dy, distance, gap))
}

pub(super) fn binding_sites(cell: &Cell) -> u32 { ContactPorts::of(cell).sites }

pub(super) fn working_contact(env: &Environment, a: &Cell, b: &Cell, max_gap: f64) -> bool {
    a.alive() && b.alive() && contact_geometry(env, a, b, max_gap).is_some()
        && binding_sites(a) > 0 && binding_sites(b) > 0
}

#[derive(Default, Clone, Copy)]
struct Store {
    stock: f64,
    capacity: f64,
    requested_in: f64,
    requested_out: f64,
    scale_in: f64,
    scale_out: f64,
    sent: f64,
    received: f64,
}
impl Store {
    fn new(stock: f64, capacity: f64) -> Self {
        Self { stock, capacity, ..Self::default() }
    }
    fn room(&self) -> f64 { (self.capacity - self.stock).max(0.0) }

    fn plan(&self, peer: &Self, max_amount: f64) -> f64 {
        if max_amount == 0.0 || self.capacity <= 0.0 || peer.capacity <= 0.0 { return 0.0; }
        let difference = self.stock / self.capacity - peer.stock / peer.capacity;
        // For an isolated pair, stop at equal fill even with very large ports.
        let equalizing = difference.abs() / (self.capacity.recip() + peer.capacity.recip());
        difference.signum() * equalizing.min(difference.abs().min(1.0) * max_amount)
    }

    fn set_scales(&mut self, throughput: f64) {
        let total = self.requested_in + self.requested_out;
        if total == 0.0 { return; }
        let port_scale = (throughput / total).min(1.0);
        if self.requested_in > 0.0 { self.scale_in = port_scale.min(self.room() / self.requested_in); }
        if self.requested_out > 0.0 { self.scale_out = port_scale.min(self.stock / self.requested_out); }
    }
}

struct JunctionEdge {
    a: usize,
    b: usize,
    /// Positive from a to b, negative from b to a.
    request: [f64; 3],
}
#[derive(Default)]
pub(super) struct JunctionWorkspace {
    ports: Vec<ContactPorts>,
    edges: Vec<JunctionEdge>,
    accounts: Vec<JunctionAccount>,
    candidates: Vec<BondCandidate>,
}
struct BondCandidate {
    rows: (usize, usize),
    ids: (u32, u32),
    previous_stage: u8,
    geometry: (f64, f64, f64, f64),
}

struct JunctionAccount {
    cell: usize,
    stores: [Store; 3],
    enrichment: f64,
    extra_sent: f64,
    extra_received: f64,
}
impl JunctionWorkspace {
    fn account(&mut self, index: usize, cell: &Cell) -> usize {
        let port = &mut self.ports[index];
        if port.account == 0 {
            let caps = cell.body.capacities();
            self.accounts.push(JunctionAccount {
                cell: index,
                enrichment: cell.reserve_enrichment, extra_sent: 0.0, extra_received: 0.0,
                stores: [Store::new(cell.energy, caps.energy), Store::new(cell.nutrient_stored, caps.minerals),
                    Store::new(cell.organic_carbon, caps.energy / CHEMICAL_ENERGY_PER_CARBON)],
            });
            port.account = self.accounts.len();
        }
        port.account - 1
    }

    fn connect(&mut self, i: usize, j: usize, a: &Cell, b: &Cell) {
        if !(0..3).any(|k| self.ports[i].junction[k] > 0.0 && self.ports[j].junction[k] > 0.0) { return; }
        let (left, right) = (self.account(i, a), self.account(j, b));
        let (a, b) = if a.id < b.id { (left, right) } else { (right, left) };
        self.edges.push(JunctionEdge { a, b, request: [0.0; 3] });
    }

    fn share(&mut self, cells: &mut [Cell], geometry: &mut CellGeometry) {
        if self.edges.is_empty() { return; }
        self.edges.sort_unstable_by_key(|edge|
            (cells[self.accounts[edge.a].cell].id, cells[self.accounts[edge.b].cell].id));
        // All edge requests use the same inventory snapshot. The total throughput
        // budget also covers stores receiving from one side and donating to another.
        for edge in &mut self.edges {
            for (k, rate) in JUNCTION_RATES.into_iter().enumerate() {
                let a = self.ports[self.accounts[edge.a].cell].junction[k];
                let b = self.ports[self.accounts[edge.b].cell].junction[k];
                let weaker = a.min(b);
                let amount = self.accounts[edge.a].stores[k].plan(&self.accounts[edge.b].stores[k], weaker * rate * DT);
                edge.request[k] = amount;
                let (from, to) = if amount >= 0.0 { (edge.a, edge.b) } else { (edge.b, edge.a) };
                self.accounts[from].stores[k].requested_out += amount.abs();
                self.accounts[to].stores[k].requested_in += amount.abs();
            }
        }
        for account in &mut self.accounts {
            for (k, rate) in JUNCTION_RATES.into_iter().enumerate() {
                account.stores[k].set_scales(self.ports[account.cell].junction[k] * rate * DT);
            }
        }
        for edge in &self.edges {
            for (k, &amount) in edge.request.iter().enumerate() {
                if amount == 0.0 { continue; }
                let (from, to) = if amount > 0.0 { (edge.a, edge.b) } else { (edge.b, edge.a) };
                let (donor, recipient) = (&self.accounts[from].stores[k], &self.accounts[to].stores[k]);
                // Residual checks guard against a rounded proportional sum a few
                // ulps above the initial inventory or available room.
                let transfer = (amount.abs() * donor.scale_out.min(recipient.scale_in))
                    .min((donor.stock - donor.sent).max(0.0))
                    .min((recipient.room() - recipient.received).max(0.0));
                if k == 2 && transfer > 0.0 && self.accounts[from].enrichment > 0.0 {
                    let donor = &self.accounts[from];
                    let left = (donor.enrichment - donor.extra_sent).max(0.0);
                    let remaining_carbon = donor.stores[k].stock - donor.stores[k].sent;
                    let extra = if transfer >= remaining_carbon { left }
                        else { (donor.enrichment * (transfer / donor.stores[k].stock)).min(left) };
                    self.accounts[from].extra_sent += extra;
                    self.accounts[to].extra_received += extra;
                }
                self.accounts[from].stores[k].sent += transfer;
                self.accounts[to].stores[k].received += transfer;
            }
        }
        for account in &self.accounts {
            let cell = &mut cells[account.cell];
            cell.reserve_enrichment = (account.enrichment - account.extra_sent).max(0.0) + account.extra_received;
            let balances = [&mut cell.energy, &mut cell.nutrient_stored, &mut cell.organic_carbon];
            for (k, (balance, store)) in balances.into_iter().zip(&account.stores).enumerate() {
                if store.received != 0.0 || store.sent != 0.0 {
                    // Preserve pre-existing overfill until the ordinary spill phase.
                    *balance = ((store.stock - store.sent).max(0.0) + store.received)
                        .min(store.stock.max(store.capacity));
                    cell.junction_net[k] = store.received - store.sent;
                }
            }
            // Capture and target selection consume this geometry later in the same tick.
            if cell.energy <= 0.0 { geometry.alive[account.cell] = false; }
        }
    }
}

impl World {
    pub(super) fn apply_adhesion_in(&mut self, workspace: &mut PhaseWorkspace) {
        if self.had_adhesive_contacts {
            for cell in &mut self.cells {
                cell.adhered_neighbors = 0;
                cell.junction_net = [0.0; 3];
            }
            self.had_adhesive_contacts = false;
        }
        let previous = std::mem::take(&mut self.contacts);
        let signals = &workspace.signals;
        if previous.is_empty() && !signals.iter().any(|s| s.recruits && s.adhesion.is_some() && s.published.is_some()) {
            return;
        }

        let junctions = &mut workspace.junctions;
        junctions.ports.clear();
        junctions.edges.clear();
        junctions.accounts.clear();
        junctions.candidates.clear();
        let grid = &mut workspace.cell_grid;
        grid.configure(self.environment.width, self.environment.height);
        grid.clear();
        let mut max_radius: f64 = 0.0;
        for (index, (cell, signal)) in self.cells.iter().zip(signals).enumerate() {
            let can_attach = cell.alive() && signal.adhesion.is_some() && signal.published.is_some();
            let ports = if can_attach { ContactPorts::of(cell) } else { ContactPorts::default() };
            junctions.ports.push(ports);
            if ports.sites > 0 {
                grid.insert(cell.x, cell.y, index);
                max_radius = max_radius.max(cell.radius());
            }
        }
        for i in 0..junctions.ports.len() {
            if junctions.ports[i].sites == 0 { continue; }
            let a = &self.cells[i];
            let search = grid.nearby_radius(a.x, a.y, a.radius() + max_radius + RELEASE_GAP);
            for j in search {
                if j <= i { continue; }
                let b = &self.cells[j];
                let (sa, sb) = (signals[i], signals[j]);
                if !sa.accepts(sb) { continue; }
                let key = (a.id.min(b.id), a.id.max(b.id));
                let old = previous.get(&key).copied().unwrap_or(0);
                if old != CLONAL && !(sa.recruits && sb.recruits) { continue; }
                let max_gap = if old >= MATURE { RELEASE_GAP } else { CONTACT_GAP };
                if let Some(geometry) = contact_geometry(&self.environment, a, b, max_gap) {
                    junctions.candidates.push(BondCandidate {
                        rows: (i, j), ids: key, previous_stage: old, geometry,
                    });
                }
            }
        }
        // Mature contacts hold their sites. Longer uninterrupted trials precede
        // new arrivals. Stable organism IDs break ties independently of grid order.
        junctions.candidates.sort_unstable_by_key(|c|
            (std::cmp::Reverse(c.previous_stage.min(MATURE)), c.ids));
        for k in 0..junctions.candidates.len() {
            let candidate = &junctions.candidates[k];
            let (i, j) = candidate.rows;
            if junctions.ports[i].sites == 0 || junctions.ports[j].sites == 0 { continue; }
            junctions.ports[i].sites -= 1;
            junctions.ports[j].sites -= 1;
            let old = candidate.previous_stage;
            let progress = if old >= MATURE { old } else { old + 1 };
            self.contacts.insert(candidate.ids, progress);
            if progress < MATURE { continue; }
            let (dx, dy, distance, gap) = candidate.geometry;
            let (before, after) = self.cells.split_at_mut(j);
            let (a, b) = (&mut before[i], &mut after[0]);
            self.had_adhesive_contacts = true;
            a.adhered_neighbors += 1;
            b.adhered_neighbors += 1;
            junctions.connect(i, j, a, b);

            // Dampen sliding and restore surface contact without pulling an
            // overlapping pair closer. Collision response owns repulsion.
            let nx = dx / distance;
            let ny = dy / distance;
            let (vx, vy) = (b.vx - a.vx, b.vy - a.vy);
            let radial = vx * nx + vy * ny;
            let pull = (radial + GAP_RELAXATION * gap.max(0.0)).max(0.0);
            let change_x = nx * pull + SHEAR_DAMPING * (vx - radial * nx);
            let change_y = ny * pull + SHEAR_DAMPING * (vy - radial * ny);
            let magnitude = change_x.hypot(change_y);
            if magnitude == 0.0 { continue; }
            let limit = ACCELERATION_PER_SITE * DT;
            let scale = (limit / magnitude).min(1.0);
            let (ma, mb) = (a.inertia(), b.inertia());
            let impulse = scale * ma * mb / (ma + mb);
            a.vx += change_x * impulse / ma;
            a.vy += change_y * impulse / ma;
            b.vx -= change_x * impulse / mb;
            b.vy -= change_y * impulse / mb;
            workspace.geometry.velocities[i] = [a.vx, a.vy];
            workspace.geometry.velocities[j] = [b.vx, b.vy];
        }
        junctions.share(&mut self.cells, &mut workspace.geometry);
    }
}

#[cfg(test)]
pub(in crate::simulation) fn advance_contact_phases(world: &mut World, genomes: &[Genome], count: u8) {
    let mut ws=PhaseWorkspace::default();
    world.freeze_contact_signals(genomes,&mut ws.signals);
    ws.geometry.rebuild(&world.cells,false,true);
    for _ in 0..count { world.apply_adhesion_in(&mut ws); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::mechanism_tests::equipped;

    fn pair() -> (World, Vec<Genome>) {
        let (mut world, genomes) = equipped("product GlueMaterial = adhesin(Frame); assembly Glue: adhere() { GlueMaterial:0.006; Mount:0.01; Wall:0.01; }\npublish rf020 = 31; adhere_message rf020 recruit;");
        let a = &mut world.cells[0];
        a.x = WIDTH - 7.25;
        a.y = 100.0;
        a.vx = -1.0;
        a.vy = 0.75;
        genomes[0].program.initialize_message(&mut a.memory);
        let mut b = a.clone();
        b.id = world.next_id;
        world.next_id += 1;
        b.x = 7.25;
        b.vx = 2.0;
        b.vy = -0.5;
        world.cells.push(b);
        (world, genomes)
    }

    fn workspace(world: &World, genomes: &[Genome]) -> PhaseWorkspace {
        let mut phases = PhaseWorkspace::default();
        world.freeze_contact_signals(genomes, &mut phases.signals);
        phases.geometry.rebuild(&world.cells, false, true);
        phases
    }

    #[test]
    fn adhesive_pairs_pull_across_the_seam_and_conserve_momentum() {
        let (mut world, genomes) = pair();
        advance_contact_phases(&mut world, &genomes, MATURE-1);
        let mut phases = workspace(&world, &genomes);
        let momentum = |w: &World| w.cells.iter().fold((0.0, 0.0), |(x, y), c| (x + c.inertia()*c.vx, y + c.inertia()*c.vy));
        let before = momentum(&world);
        world.apply_adhesion_in(&mut phases);
        assert!(world.cells[0].vx > -1.0 && world.cells[1].vx < 2.0);
        assert!(world.cells[0].vy < 0.75 && world.cells[1].vy > -0.5);
        let after = momentum(&world);
        assert!((before.0-after.0).abs() < 1e-10 && (before.1-after.1).abs() < 1e-10);
    }

    #[test]
    fn encounters_need_uninterrupted_bilateral_recruitment_before_any_effect() {
        let (mut world, genomes) = pair();
        let mut phases = workspace(&world,&genomes);
        let original = (world.cells[0].vx, world.cells[1].vx);
        phases.signals[0].recruits=false;
        for _ in 0..MATURE { world.apply_adhesion_in(&mut phases); }
        assert!(world.contacts.is_empty(), "a daughters-only founder cannot be recruited");
        phases.signals[0].recruits=true;
        for _ in 0..MATURE-1 { world.apply_adhesion_in(&mut phases); }
        assert_eq!(world.contacts.values().copied().collect::<Vec<_>>(), [MATURE-1]);
        assert_eq!(world.bond_pairs().count(),0);
        assert_eq!((world.cells[0].vx,world.cells[1].vx),original);
        assert!(world.cells.iter().all(|c|c.adhered_neighbors==0 && c.junction_net==[0.0;3]));
        assert_eq!(world.move_cell(1,world.cells[0].x,world.cells[0].y,true,&genomes).unwrap(),1);
        world.cells[1].x+=1.25;
        world.apply_adhesion_in(&mut phases);
        assert!(world.contacts.is_empty(), "a partial encounter breaks beyond the acquisition radius");
        world.cells[1].x-=1.25;
        for _ in 0..MATURE-1 { world.apply_adhesion_in(&mut phases); }
        assert_eq!(world.bond_pairs().count(),0);
        world.apply_adhesion_in(&mut phases);
        assert_eq!(world.bond_pairs().collect::<Vec<_>>(),[(1,2)]);
        world.cells[1].x+=1.25;
        world.apply_adhesion_in(&mut phases);
        assert!(world.has_bond(2,1),"a formed link tolerates a small stretch");
        phases.signals[1].adhesion=None;
        world.apply_adhesion_in(&mut phases);
        assert!(!world.has_bond(1,2),"either peer can withdraw immediately");
    }

    #[test]
    fn adhesion_requires_two_reciprocal_signals_and_working_surfaces_in_reach() {
        for condition in ["other marker", "private", "one-sided", "inactive", "damaged", "out of reach", "wall"] {
            let (mut world, genomes) = pair();
            advance_contact_phases(&mut world, &genomes, MATURE-1);
            let mut phases = workspace(&world, &genomes);
            let adhesive = world.cells[1].body.cellular.as_mut().unwrap().assemblies.last_mut().unwrap();
            match condition {
                "other marker" => phases.signals[1].published = Some(30.0),
                "private" => phases.signals[1].published = None,
                "one-sided" => phases.signals[1].adhesion = None,
                "inactive" => adhesive.activity = 0.0,
                "damaged" => adhesive.integrity = 0.0,
                "out of reach" => world.cells[1].x += 2.0,
                "wall" => { assert!(world.environment.add_barrier(0.0,100.0,30.0,10.0,PI*0.5,false) > 0); }
                _ => unreachable!(),
            }
            world.apply_adhesion_in(&mut phases);
            assert_eq!((world.cells[0].vx, world.cells[1].vx), (-1.0, 2.0), "{condition}");
        }
    }
}

#[cfg(test)]
mod junction_tests;
