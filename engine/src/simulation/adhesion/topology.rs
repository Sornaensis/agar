//! Persistent contact history is keyed by immutable organism IDs. A value
//! below MATURE records uninterrupted recruitment and grants no bond effects.
use super::*;
use std::collections::HashMap;

/// Origin determines colony membership. A recruited pair with compatible
/// installed resource ports can exchange food even when flux is currently zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(in crate::simulation) enum BondKind { Feeding = 0, Colony = 1, Contact = 2 }

fn resource_ports(cell: &Cell, bearing: f64) -> u8 {
    cell.body.cellular.iter().flat_map(|s| &s.assemblies)
        .filter(|a| a.working_mass() * a.activity > 0.0 && covers_interface(a.at(), bearing))
        .filter_map(|a| resource_channel(a.recipe.kind))
        .fold(0, |mask, k| mask | (1 << k))
}

/// Cell rows are stable throughout one tick's fission pass (only daughters
/// append). Create the sparse incidence lists once if a bonded parent divides.
pub(in crate::simulation) struct DivisionBonds {
    neighbors: Vec<Vec<usize>>,
}
impl DivisionBonds {
    pub(in crate::simulation) fn new(world:&World)->Self {
        let mut index=Self{neighbors:vec![Vec::new();world.cells.len()]};
        let rows=world.cell_rows();
        for (a,b) in world.bond_pairs() { index.add(rows[&a],rows[&b]); }
        index
    }
    pub(in crate::simulation) fn add(&mut self,a:usize,b:usize) {
        self.neighbors[a].push(b);
        self.neighbors[b].push(a);
    }
    pub(in crate::simulation) fn append_daughter(&mut self,daughter:usize) {
        debug_assert_eq!(self.neighbors.len(),daughter);
        self.neighbors.push(Vec::new());
    }
}

impl World {
    pub(crate) fn bond_pairs(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.contacts.iter().filter_map(|(&pair, &ticks)| (ticks >= MATURE).then_some(pair))
    }
    pub(in crate::simulation) fn presented_bonds(&self, endpoint: Option<u32>)
        -> impl Iterator<Item = (u32, u32, BondKind)> + '_ {
        let edges = self.contacts.iter().filter_map(move |(&(a, b), &stage)| {
            (stage >= MATURE && endpoint.is_none_or(|id| a == id || b == id))
                .then_some((a, b, stage))
        });
        let rows = edges.clone().any(|(_,_,stage)| stage != CLONAL).then(|| self.cell_rows());
        edges.map(move |(a, b, stage)| {
            let kind = if stage == CLONAL { BondKind::Colony } else {
                let rows = rows.as_ref().expect("nonclonal endpoint index");
                let (ca,cb) = (&self.cells[rows[&a]], &self.cells[rows[&b]]);
                let facing = resource_ports(ca, interface_bearing(&self.environment, ca, cb))
                    & resource_ports(cb, interface_bearing(&self.environment, cb, ca));
                if facing != 0 { BondKind::Feeding } else { BondKind::Contact }
            };
            (a, b, kind)
        })
    }
    pub(in crate::simulation) fn has_bond(&self, a: u32, b: u32) -> bool {
        self.contacts.get(&(a.min(b), a.max(b))).is_some_and(|&stage| stage >= MATURE)
    }

    pub(in crate::simulation) fn division_interface(&self, a: u32, b: u32) -> bool {
        self.contacts.get(&(a.min(b), a.max(b))) == Some(&CLONAL)
    }

    fn cell_rows(&self) -> HashMap<u32, usize> {
        self.cells.iter().enumerate().map(|(i, cell)| (cell.id, i)).collect()
    }
    pub(in crate::simulation) fn recount_bonds(&mut self) {
        let mut degrees = HashMap::<u32,u32>::new();
        for (a,b) in self.bond_pairs() {
            *degrees.entry(a).or_default() += 1;
            *degrees.entry(b).or_default() += 1;
        }
        for cell in &mut self.cells {
            cell.adhered_neighbors = degrees.get(&cell.id).copied().unwrap_or(0);
        }
        self.had_adhesive_contacts = !degrees.is_empty() || self.cells.iter().any(|c|c.junction_net != [0.0;3]);
    }

    /// A late birth, external edit, lost working surface, changed selector or
    /// dead endpoint can invalidate a pair after the physical exchange phase.
    pub(crate) fn prune_contacts(&mut self, genomes: Option<&[Genome]>) {
        if self.contacts.is_empty() {
            return;
        }
        let rows = self.cell_rows();
        let before = self.contacts.len();
        let mut available: HashMap<usize, (GlueStock, CellSignal)> = HashMap::new();
        let mut scratch = Vec::new();
        // Capacity can fall after a split, damage or regulation. The same
        // priority used for acquisition also governs retention after edits.
        let mut pairs: Vec<_> = self.contacts.iter().map(|(&ids, &stage)| (ids, stage)).collect();
        pairs.sort_unstable_by_key(|&(ids, stage)| (std::cmp::Reverse(stage.min(MATURE)), ids));
        for ((a, b), stage) in pairs {
            let keep = rows.get(&a).zip(rows.get(&b)).is_some_and(|(&i, &j)| {
                let (a, b) = (&self.cells[i], &self.cells[j]);
                if !a.alive() || !b.alive() {
                    return false;
                }
                let max_gap = if stage >= MATURE { RELEASE_GAP } else { CONTACT_GAP };
                if contact_geometry(&self.environment, a, b, max_gap).is_none() {
                    return false;
                }
                for (row, cell) in [(i, a), (j, b)] {
                    available.entry(row).or_insert_with(|| {
                        let p = genomes.map(|registry| &registry[(cell.genome_id - 1) as usize].program);
                        let mut stock = GlueStock::default();
                        stock.refill(cell, p);
                        (stock, p.map_or_else(CellSignal::default, |p| CellSignal::from_cell(cell, p)))
                    });
                }
                let pa = genomes.map(|g| &g[(a.genome_id - 1) as usize].program);
                let pb = genomes.map(|g| &g[(b.genome_id - 1) as usize].program);
                let (aa, bb) = (&available[&i], &available[&j]);
                let Some(ma) = aa.0.eligible(pa, &self.environment, (a,b), &aa.1, &bb.1, stage == CLONAL, &mut scratch) else {return false};
                let Some(mb) = bb.0.eligible(pb, &self.environment, (b,a), &bb.1, &aa.1, stage == CLONAL, &mut scratch) else {return false};
                available.get_mut(&i).unwrap().0.claim(ma);
                available.get_mut(&j).unwrap().0.claim(mb);
                true
            });
            if !keep {
                self.contacts.remove(&(a, b));
            }
        }
        if self.contacts.len() != before {
            self.recount_bonds();
        }
    }

    /// Reserve each working recipe's material for the other mature interfaces
    /// before asking whether this particular incoming partner can use a site.
    fn free_site_for(
        &self,
        endpoint: usize,
        incoming: usize,
        replacing: Option<usize>,
        division: bool,
        genomes: &[Genome],
        index: Option<&DivisionBonds>,
    ) -> bool {
        let cell = &self.cells[endpoint];
        if binding_sites(cell) == 0 {
            return false;
        }
        let program = &genomes[(cell.genome_id - 1) as usize].program;
        let local = CellSignal::from_cell(cell, program);
        let mut stock = GlueStock::default();
        stock.refill(cell, Some(program));
        let mut scratch = Vec::new();
        if let Some(index) = index {
            let mut peers = index.neighbors[endpoint].clone();
            peers.sort_unstable_by_key(|&row| self.cells[row].id);
            for row in peers {
                if Some(row) == replacing {
                    continue;
                }
                let peer = &self.cells[row];
                let stage = self.contacts[&(cell.id.min(peer.id), cell.id.max(peer.id))];
                let other = CellSignal::from_cell(peer, &genomes[(peer.genome_id - 1) as usize].program);
                let mask = stock.eligible(Some(program), &self.environment, (cell,peer), &local, &other, stage == CLONAL, &mut scratch);
                if let Some(mask) = mask {
                    stock.claim(mask);
                }
            }
        }
        let peer = &self.cells[incoming];
        let signal = CellSignal::from_cell(peer, &genomes[(peer.genome_id - 1) as usize].program);
        stock.eligible(Some(program), &self.environment, (cell,peer), &local, &signal, division, &mut scratch).is_some()
    }

    pub(in crate::simulation) fn can_retain_cleavage(
        &self,
        parent: usize,
        daughter: usize,
        genomes: &[Genome],
        index: Option<&DivisionBonds>,
    ) -> bool {
        let (a, b) = (&self.cells[parent], &self.cells[daughter]);
        a.alive() && b.alive()
            && genomes[(a.genome_id - 1) as usize].program.has_adhesion_guard()
            && genomes[(b.genome_id - 1) as usize].program.has_adhesion_guard()
            && a.adhered_neighbors < binding_sites(a)
            && b.adhered_neighbors < binding_sites(b)
            && contact_geometry(&self.environment, a, b, CONTACT_GAP).is_some()
            && self.free_site_for(parent, daughter, None, true, genomes, index)
            && self.free_site_for(daughter, parent, None, true, genomes, index)
    }

    /// The parent's old adhesive interface can physically land on the other
    /// half of a split. Keep one copy, with the same known partner and origin.
    pub(in crate::simulation) fn inherit_bond_interfaces(
        &mut self,
        parent: usize,
        daughter: usize,
        genomes: &[Genome],
        index: &mut DivisionBonds,
    ) {
        let parent_id = self.cells[parent].id;
        let daughter_id = self.cells[daughter].id;
        let mut peers = index.neighbors[parent].clone();
        peers.sort_unstable_by_key(|&i| self.cells[i].id);
        let mut stocks = [GlueStock::default(), GlueStock::default()];
        let rows = [parent, daughter];
        let programs = rows.map(|row| &genomes[(self.cells[row].genome_id - 1) as usize].program);
        let locals = rows.map(|row| {
            CellSignal::from_cell(&self.cells[row], &genomes[(self.cells[row].genome_id - 1) as usize].program)
        });
        for (n, stock) in stocks.iter_mut().enumerate() {
            stock.refill(&self.cells[rows[n]], Some(programs[n]));
        }
        let mut scratch = Vec::new();
        for peer in peers {
            if peer == daughter {
                continue;
            }
            let peer_id = self.cells[peer].id;
            let pair = (parent_id.min(peer_id), parent_id.max(peer_id));
            let stage = self.contacts[&pair];
            let p = &self.cells[peer];
            let ps = CellSignal::from_cell(p, &genomes[(p.genome_id - 1) as usize].program);
            let mut claimed = None;
            for n in 0..2 {
                if !working_contact(&self.environment, &self.cells[rows[n]], p, RELEASE_GAP) {
                    continue;
                }
                let Some(mask) = stocks[n].eligible(Some(programs[n]), &self.environment, (&self.cells[rows[n]],p), &locals[n], &ps, stage == CLONAL, &mut scratch) else { continue };
                if self.free_site_for(peer, rows[n], Some(parent), stage == CLONAL, genomes, Some(index))
                {
                    stocks[n].claim(mask);
                    claimed = Some(rows[n]);
                    break;
                }
            }
            if claimed == Some(parent) {
                continue;
            }
            let transfer = claimed == Some(daughter);
            self.contacts.remove(&pair);
            index.neighbors[parent].retain(|&j| j != peer);
            index.neighbors[peer].retain(|&j| j != parent);
            self.cells[parent].adhered_neighbors -= 1;
            if transfer {
                index.add(daughter, peer);
                self.contacts.insert((peer_id.min(daughter_id), peer_id.max(daughter_id)), stage);
                self.cells[daughter].adhered_neighbors += 1;
            } else {
                self.cells[peer].adhered_neighbors -= 1;
            }
        }
    }

    pub(crate) fn validate_or_migrate_contacts(&mut self, genomes: &[Genome], version: u32) -> Result<(),String> {
        if version < 16 {
            // Earlier formats have no finished adhesin. They retain all paid
            // hardware, but cannot retain a chemical bond or pending site.
            if version == 15 {
                let saved: Vec<_> = self.cells.iter().map(|c| c.adhered_neighbors).collect();
                self.recount_bonds();
                if self.cells.iter().zip(saved).any(|(c,n)| c.adhered_neighbors != n) {
                    return Err("Invalid simulation save: historical adhesion counts disagree with pairs".into());
                }
            }
            self.contacts.clear();
            for cell in &mut self.cells {
                cell.adhered_neighbors = 0;
                cell.junction_net = [0.0; 3];
            }
            self.had_adhesive_contacts = false;
            return Ok(());
        }

        let before = self.contacts.len();
        self.prune_contacts(Some(genomes));
        if version >= 20 && self.contacts.len() != before { return Err("Invalid simulation save: incompatible adhesion endpoints".into()); }
        // Founders persist in the immutable parent chain only while alive.
        // Reject provably distinct origins; a deceased ancestor need not
        // remain as a living endpoint after a legitimate interface handoff.
        if self.contacts.values().any(|&stage|stage==CLONAL) {
            let rows=self.cell_rows();
            let mut roots: HashMap<u32,Option<u32>>=HashMap::new();
            let mut ids: Vec<_>=rows.keys().copied().collect(); ids.sort_unstable();
            for id in ids {
                let parent=self.cells[rows[&id]].parent_id;
                let root=if parent==0 {Some(id)} else {roots.get(&parent).copied().flatten()};
                roots.insert(id,root);
            }
            for (&(a,b),&stage) in &self.contacts {
                if stage==CLONAL && matches!((roots[&a],roots[&b]),(Some(a),Some(b)) if a!=b) {
                    return Err("Invalid simulation save: retained bond joins distinct founders".into());
                }
            }
        }
        let saved: Vec<_> = self.cells.iter().map(|c| c.adhered_neighbors).collect();
        self.recount_bonds();
        if self.cells.iter().zip(saved).any(|(c, degree)| c.adhered_neighbors != degree) {
            return Err("Invalid simulation save: adhesion counts disagree with confirmed pairs".into());
        }
        Ok(())
    }
}
