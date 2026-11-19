//! Persistent contact history is keyed by immutable organism IDs. A value
//! below MATURE records uninterrupted recruitment and grants no bond effects.
use super::*;
use std::collections::HashMap;

fn direct_family(a: &Cell, b: &Cell) -> bool { a.parent_id == b.id || b.parent_id == a.id }

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
    fn add(&mut self,a:usize,b:usize) {
        self.neighbors[a].push(b);
        self.neighbors[b].push(a);
    }
    pub(in crate::simulation) fn append_daughter(&mut self,world:&World,parent:usize,daughter:usize) {
        debug_assert_eq!(self.neighbors.len(),daughter);
        self.neighbors.push(Vec::new());
        if world.has_bond(world.cells[parent].id,world.cells[daughter].id) { self.add(parent,daughter); }
    }
}

impl World {
    pub(crate) fn bond_pairs(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.contacts.iter().filter_map(|(&pair, &ticks)| (ticks >= MATURE).then_some(pair))
    }
    pub(in crate::simulation) fn has_bond(&self, a: u32, b: u32) -> bool {
        self.contacts.get(&(a.min(b), a.max(b))).is_some_and(|&stage| stage >= MATURE)
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
        if self.contacts.is_empty() { return; }
        let rows = self.cell_rows();
        let before = self.contacts.len();
        self.contacts.retain(|&(a,b), ticks| {
            let (Some(&i),Some(&j)) = (rows.get(&a),rows.get(&b)) else { return false; };
            let (a,b) = (&self.cells[i],&self.cells[j]);
            let mature = *ticks >= MATURE;
            if let Some(genomes) = genomes {
                let sa=CellSignal::from_cell(a,&genomes[(a.genome_id-1) as usize].program);
                let sb=CellSignal::from_cell(b,&genomes[(b.genome_id-1) as usize].program);
                if !sa.accepts(sb) || !(*ticks == CLONAL || sa.recruits && sb.recruits) { return false; }
            }
            working_contact(&self.environment, a, b, if mature {RELEASE_GAP} else {CONTACT_GAP})
        });
        if self.contacts.len() != before { self.recount_bonds(); }
    }

    /// The parent's old adhesive interface can physically land on the other
    /// half of a split. Keep one copy, with the same known partner and origin.
    pub(in crate::simulation) fn inherit_bond_interfaces(&mut self, parent: usize, daughter: usize, genomes: &[Genome], index:&mut DivisionBonds) {
        let parent_id=self.cells[parent].id;
        let daughter_id=self.cells[daughter].id;
        for peer in index.neighbors[parent].clone() {
            if peer == daughter { continue; }
            let peer_id=self.cells[peer].id;
            let pair=(parent_id.min(peer_id),parent_id.max(peer_id));
            let stage=self.contacts[&pair];
            let p=&self.cells[peer];
            let ps=CellSignal::from_cell(p,&genomes[(p.genome_id-1) as usize].program);
            let eligible=|c:&Cell|{
                let cs=CellSignal::from_cell(c,&genomes[(c.genome_id-1) as usize].program);
                cs.accepts(ps) && (stage==CLONAL || cs.recruits && ps.recruits)
                    && working_contact(&self.environment,c,p,RELEASE_GAP)
            };
            if !eligible(&self.cells[parent]) && eligible(&self.cells[daughter]) {
                self.contacts.remove(&pair);
                index.neighbors[parent].retain(|&j|j!=peer);
                index.neighbors[peer].retain(|&j|j!=parent);
                index.add(daughter,peer);
                self.contacts.insert((peer_id.min(daughter_id),peer_id.max(daughter_id)),stage);
                self.cells[parent].adhered_neighbors-=1;
                self.cells[daughter].adhered_neighbors+=1;
            }
        }
    }

    pub(crate) fn validate_or_migrate_contacts(&mut self, genomes: &[Genome], version: u32) -> Result<(),String> {
        if version >= 15 {
            let before = self.contacts.len();
            self.prune_contacts(Some(genomes));
            if self.contacts.len() != before { return Err("Invalid simulation save: incompatible adhesion endpoints".into()); }
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
            return Ok(());
        }
        if !self.cells.iter().any(|c| c.adhered_neighbors != 0) { return Ok(()); }
        // Old saves had counts but no partner IDs. Keep only real direct
        // mother/daughter pairs for which each endpoint's complete eligible
        // neighborhood uniquely accounts for its recorded count, all kin.
        let mut signals = Vec::new();
        self.freeze_contact_signals(genomes,&mut signals);
        let mut candidates = Vec::new();
        let mut degrees = vec![0u32; self.cells.len()];
        let mut family_only = vec![true; self.cells.len()];
        let mut grid = Grid::default();
        grid.configure(self.environment.width,self.environment.height);
        let mut largest: f64 = 0.0;
        let active: Vec<_> = self.cells.iter().map(|c| ContactPorts::of(c).adhesive>0.0).collect();
        for (i,cell) in self.cells.iter().enumerate() {
            if active[i] { grid.insert(cell.x,cell.y,i); largest=largest.max(cell.radius()); }
        }
        for (i, (cell, signal)) in self.cells.iter().zip(&signals).enumerate() {
            if !active[i] || signal.adhesion.is_none() { continue; }
            for j in grid.nearby_radius(cell.x,cell.y,cell.radius()+largest+CONTACT_GAP) {
                if i >= j || !signal.accepts(signals[j]) { continue; }
                let peer=&self.cells[j];
                if contact_geometry(&self.environment,cell,peer,CONTACT_GAP).is_none() { continue; }
                degrees[i]+=1; degrees[j]+=1;
                if direct_family(cell,peer) { candidates.push((i,j)); }
                else { family_only[i]=false; family_only[j]=false; }
            }
        }
        for (i,j) in candidates {
            if [i,j].into_iter().all(|k| family_only[k] && degrees[k] == self.cells[k].adhered_neighbors) {
                let (a,b)=(self.cells[i].id,self.cells[j].id);
                self.contacts.insert((a.min(b),a.max(b)),CLONAL);
            }
        }
        self.recount_bonds();
        for c in &mut self.cells {
            if c.adhered_neighbors == 0 { c.junction_net = [0.0; 3]; }
        }
        Ok(())
    }
}
