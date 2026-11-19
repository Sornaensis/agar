//! Player interventions run between complete ticks. Placement is a relocation
//! to a clear destination, independent of the path the pointer took there.
use super::*;

type Point = (f64, f64);

impl World {
    fn edit_point(&self, x: f64, y: f64) -> Result<Point, String> {
        if !x.is_finite() || !y.is_finite() { return Err("Choose finite world coordinates.".into()); }
        // rem_euclid can round a tiny negative remainder up to the period.
        let coord = |v: f64, size: f64| { let n = wrap(v, size); if n == size { 0.0 } else { n } };
        Ok((coord(x, self.environment.width), coord(y, self.environment.height)))
    }

    fn check_space(&self, point: Point, radius: f64, others: impl Iterator<Item = usize>) -> Result<(), String> {
        if !self.environment.circle_fits(point, radius) {
            return Err("The organism would overlap a barrier at this location.".into());
        }
        for i in others {
            let other = &self.cells[i];
            let dx = delta(point.0 - other.x, self.environment.width);
            let dy = delta(point.1 - other.y, self.environment.height);
            if dx.hypot(dy) < radius + other.radius() {
                return Err("Another organism occupies this location.".into());
            }
        }
        Ok(())
    }

    pub fn place_cell(&mut self, genome: u32, x: f64, y: f64, genomes: &[Genome]) -> Result<u32, String> {
        let program = Self::founder_program(genome, genomes)?;
        let point = self.edit_point(x, y)?;
        let radius = CELL_RADIUS * crate::body::FOUNDER_AREA.sqrt();
        self.check_space(point, radius, 0..self.cells.len())?;
        let id = self.next_id;
        self.seed_bodies(genome, 1, FounderPlacement::Exact(point.0, point.1), Body::founder, Some(program))?;
        Ok(id)
    }

    fn living_index(&self, id: u32) -> Result<usize, String> {
        self.cells.iter().position(|cell| cell.id == id && cell.alive())
            .ok_or_else(|| "This organism is no longer alive.".into())
    }

    pub fn move_cell(&mut self, id: u32, x: f64, y: f64, colony: bool, genomes: &[Genome]) -> Result<u32, String> {
        let seed = self.living_index(id)?;
        let destination = self.edit_point(x, y)?;
        let origin = (self.cells[seed].x, self.cells[seed].y);
        let translation = (delta(destination.0 - origin.0, self.environment.width),
            delta(destination.1 - origin.1, self.environment.height));
        let mut grid = Grid::default();
        grid.configure(self.environment.width, self.environment.height);
        let mut largest: f64 = 0.0;
        for (i, cell) in self.cells.iter().enumerate() {
            grid.insert(cell.x, cell.y, i);
            largest = largest.max(cell.radius());
        }
        let mut group = vec![seed];
        let mut selected = vec![false; self.cells.len()];
        selected[seed] = true;
        let mut bonds = Vec::new();
        if colony {
            let mut signals = Vec::new();
            self.freeze_contact_signals(genomes, &mut signals);
            if !signals.is_empty() {
                let mut cursor = 0;
                while cursor < group.len() {
                    let i = group[cursor];
                    cursor += 1;
                    let cell = &self.cells[i];
                    for j in grid.nearby_radius(cell.x, cell.y, largest + cell.radius() + adhesion::CONTACT_GAP) {
                        if i == j || !adhesion::bonded(&self.environment, cell, &self.cells[j], signals[i], signals[j]) { continue; }
                        if i < j { bonds.push((i, j)); }
                        if !selected[j] { selected[j] = true; group.push(j); }
                    }
                }
            }
        }
        if origin == destination { return Ok(group.len() as u32); }
        let mut positions = Vec::with_capacity(group.len());
        let point = |i: usize| -> Result<Point, String> {
            if i == seed { Ok(destination) }
            else { self.edit_point(self.cells[i].x + translation.0, self.cells[i].y + translation.1) }
        };
        for &i in &group {
            let target = point(i)?;
            let radius = self.cells[i].radius();
            self.check_space(target, radius, grid.nearby_radius(target.0, target.1, largest + radius)
                .filter(|&j| !selected[j]))?;
            positions.push(target);
        }
        for (a, b) in bonds {
            if self.environment.segment_blocked(point(a)?, point(b)?) {
                return Err("A barrier would separate attached organisms at this location.".into());
            }
        }
        // A single-body edit can break the last observed exchange. Discard its
        // historical contact indicators and those of former local peers so a
        // paused frame cannot show a remote bond. A rigidly moved component
        // retains the transfers that actually occurred between its members.
        if !colony {
            self.invalidate_local_exchange(seed, grid.nearby_radius(origin.0, origin.1,
                largest + self.cells[seed].radius() + adhesion::CONTACT_GAP));
        }
        for (i, (x, y)) in group.iter().copied().zip(positions) {
            self.cells[i].x = x;
            self.cells[i].y = y;
        }
        Ok(group.len() as u32)
    }

    fn invalidate_local_exchange(&mut self, seed: usize, nearby: impl Iterator<Item = usize>) {
        let source = &self.cells[seed];
        if source.adhered_neighbors == 0 { return; }
        let (x, y, radius) = (source.x, source.y, source.radius());
        for i in nearby {
            let cell = &mut self.cells[i];
            let dx = delta(cell.x - x, self.environment.width);
            let dy = delta(cell.y - y, self.environment.height);
            if cell.adhered_neighbors != 0 && dx.hypot(dy) <= radius + cell.radius() + adhesion::CONTACT_GAP {
                cell.adhered_neighbors = 0;
                cell.junction_net = [0.0; 3];
            }
        }
    }

    pub fn set_cell_stores(&mut self, id: u32, energy: Option<f64>, minerals: Option<f64>) -> Result<(), String> {
        let i = self.living_index(id)?;
        if energy.is_none() && minerals.is_none() { return Err("Choose an energy or mineral amount to change.".into()); }
        let cell = &self.cells[i];
        let caps = cell.body.capacities();
        let valid = |n: f64, cap: f64| n.is_finite() && n >= 0.0 && n <= cap;
        if energy.is_some_and(|n| !valid(n, caps.energy)) {
            return Err(format!("Available ATP must be between 0 and {}.", caps.energy));
        }
        if minerals.is_some_and(|n| !valid(n, caps.minerals)) {
            return Err(format!("Stored minerals must be between 0 and {}.", caps.minerals));
        }
        if energy == Some(0.0) {
            // The normal tick preflights packet storage and IDs before mutating
            // the world. The same guarantee applies when an edit triggers death.
            let potential = cell.food_potential();
            let organic_energy = potential.body + potential.reserve + potential.gut;
            self.reserve_organic_packets(organic_energy, 4)?;
        }
        let cell = &mut self.cells[i];
        if let Some(n) = energy {
            self.energy_imports += (n - cell.energy).max(0.0);
            self.energy_withdrawn += (cell.energy - n).max(0.0);
            cell.energy = n;
        }
        if let Some(n) = minerals {
            self.nutrient_imports += (n - cell.nutrient_stored).max(0.0);
            self.mineral_withdrawn += (cell.nutrient_stored - n).max(0.0);
            cell.nutrient_stored = n;
        }
        if !cell.alive() {
            // Removal, gas release and carcass formation use the ordinary lifecycle.
            self.invalidate_local_exchange(i, 0..self.cells.len());
            self.remove_dead_in(&mut PhaseWorkspace::default());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::mechanism_tests::equipped;
    use crate::checkpoint::{Reader, Writer};

    fn bytes(world: &World) -> Vec<u8> {
        let mut w = Writer::new();
        world.write_checkpoint(&mut w);
        w.0
    }

    #[test]
    fn exact_placement_is_an_ordinary_funded_founder_and_rejection_preserves_rng() {
        let mut world = World::empty(455);
        let source = "cellular; publish rf001 = 7; product Frame=scaffold();";
        let genomes = vec![Genome::new(source.into(), Program::compile(source).unwrap(), 0, 0)];
        let id = world.place_cell(1, -3.125, 80.0625, &genomes).unwrap();
        assert_eq!(id, 1);
        assert_eq!((world.cells[0].x,world.cells[0].y), (WIDTH - 3.125, 80.0625));
        assert_eq!(genomes[0].program.published_message(&world.cells[0].memory), Some(7.0));
        assert_eq!(world.births, 0);
        assert_eq!(world.cells[0].body.organs.len(), 1);
        assert_eq!(world.cells[0].energy,68.0);
        assert!(world.environment.add_barrier(100.0,100.0,100.0,20.0,0.0,false)>0);
        let before = bytes(&world);
        for (genome,x,y) in [(1,1.0,80.0),(1,100.0,100.0),(2,300.0,100.0),(1, f64::NAN, 100.0)] {
            assert!(world.place_cell(genome, x, y, &genomes).is_err());
            assert_eq!(bytes(&world), before);
        }
        let next = world.place_cell(1, 410.25, 12.5,&genomes).unwrap();
        assert_eq!(next,2);
    }

    #[test]
    fn moves_connected_component_across_seams_and_rejects_any_blocked_member() {
        let (mut world, genomes) = equipped("assembly Glue: adhere() { Mount:0.01; Wall:0.01; }\npublish rf020 = 31; adhere_message rf020;");
        genomes[0].program.initialize_message(&mut world.cells[0].memory);
        world.cells[0].x = WIDTH - 13.0;
        world.cells[0].y = 50.0;
        for x in [1.5,16.0,60.0] {
            let mut member = world.cells[0].clone();
            member.id = world.next_id;
            world.next_id += 1;
            member.x = x;
            world.cells.push(member);
        }
        let pts = |w:&World| w.cells.iter().map(|c|(c.x,c.y)).collect::<Vec<_>>();
        assert_eq!(world.move_cell(2,10.0,101.0,true,&genomes).unwrap(),3);
        assert_eq!(pts(&world),vec![(WIDTH-4.5,101.0),(10.0,101.0),(24.5,101.0),(60.0,50.0)]);
        let before = bytes(&world);
        assert!(world.move_cell(2,45.5,50.0,true,&genomes).is_err());
        assert_eq!(bytes(&world),before);
        assert!(world.environment.add_barrier(350.0,101.0,20.0,10.0,0.0,false)>0);
        let before = bytes(&world);
        assert!(world.move_cell(2,324.0,101.0,true,&genomes).is_err());
        assert_eq!(bytes(&world),before);
        for (member, neighbors) in world.cells[..3].iter_mut().zip([1,2,1]) {
            member.adhered_neighbors = neighbors;
            member.junction_net = [0.4,0.0,0.0];
        }
        world.had_adhesive_contacts = true;
        assert_eq!(world.move_cell(2,130.0,80.0,false,&genomes).unwrap(),1);
        assert_eq!(pts(&world)[2],(24.5,101.0));
        assert!(world.cells[..3].iter().all(|c| c.adhered_neighbors == 0 && c.junction_net == [0.0;3]));
        // A former neighbor cannot drag its old component after a single-cell edit.
        assert_eq!(world.move_cell(2,140.0,80.0,true,&genomes).unwrap(),1);
    }

    #[test]
    fn store_edits_conserve_matter_survive_restore_and_zero_atp_uses_real_death() {
        let mut world = World::empty(42);
        let source = "thrust=0;turn=0;divide=false;";
        let genomes = vec![Genome::new(source.into(),Program::compile(source).unwrap(),0,0)];
        let id = world.place_cell(1,250.0,200.0,&genomes).unwrap();
        let initial = (world.nutrient_input_total(),world.energy_input_total());
        let before = bytes(&world);
        for (e,m) in [(Some(1.0),Some(-1.0)),(Some(f64::NAN),None),(None,None),(None,Some(f64::INFINITY))] {
            assert!(world.set_cell_stores(id,e,m).is_err());
            assert_eq!(bytes(&world),before);
        }
        world.set_cell_stores(id,Some(70.0),Some(0.7)).unwrap();
        assert!((world.nutrient_input_total()-initial.0-0.1).abs()<1e-9);
        assert_eq!(world.energy_input_total(), initial.1+2.0);
        world.set_cell_stores(id,Some(34.0),Some(0.5)).unwrap();
        assert_eq!(world.energy_withdrawn,36.0);
        assert!((world.mineral_withdrawn-0.2).abs()<1e-9);
        let stored = bytes(&world);
        let mut loaded = World::read_checkpoint(&mut Reader::new(&stored),1,crate::checkpoint::VERSION).unwrap();
        assert_eq!(bytes(&loaded),stored);
        let next_food_id = loaded.next_food_id;
        loaded.next_food_id = u64::MAX / 2;
        let exhausted = bytes(&loaded);
        assert!(loaded.set_cell_stores(id, Some(0.0), Some(0.0)).is_err());
        assert_eq!(bytes(&loaded), exhausted);
        loaded.next_food_id = next_food_id;
        let carbon = loaded.carbon_total();
        loaded.set_cell_stores(id,Some(0.0),None).unwrap();
        assert!(loaded.cells.is_empty());
        assert_eq!(loaded.deaths,1);
        assert!(!loaded.food.is_empty());
        assert!((loaded.carbon_total()-carbon).abs()<1e-8);
        assert_eq!(loaded.energy_withdrawn,70.0);
        assert!((loaded.nutrient_total()+loaded.mineral_withdrawn-loaded.nutrient_input_total()).abs()<1e-8);
        let after = bytes(&loaded);
        World::read_checkpoint(&mut Reader::new(&after),1,crate::checkpoint::VERSION).unwrap();
        assert!(loaded.set_cell_stores(id,Some(1.0),None).is_err());
        assert_eq!(bytes(&loaded),after);
    }
}
