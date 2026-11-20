//! Whole-count resource transactions, used by the ordinary action/physics loop.
use crate::counts::CARBON_ENERGY;
use super::*;
use crate::counts::{self, CountMeal, CellCounts, FoodCounts, units, wide_units, floor, nearest, proportional, rate};

impl Cell {
    pub(crate) fn project_counts(&mut self) {
        let Some(s) = self.counted.as_ref() else { return; };
        self.energy = units(s.atp);
        self.nutrient_stored = units(s.mineral);
        self.organic_carbon = units(s.carbon);
        self.internal_oxygen = units(s.oxygen);
        self.internal_inorganic_carbon = units(s.dic);
        self.reserve_enrichment = units(s.reserve_extra);
        self.structural_enrichment = units(s.structural_extra);
        self.gut = s.gut.map(CountMeal::project);
        self.refresh_gut_totals();
    }
}
impl CountMeal {
    fn project(self) -> OrganicMatter {
        OrganicMatter { energy: wide_units(self.energy()), nutrients: units(self.mineral), extra_energy: units(self.extra) }
    }
}
impl Food {
    fn project_counts(&mut self) {
        if let Some(q) = self.counted {
            self.energy = wide_units(q.meal.energy());
            self.nutrients = units(q.meal.mineral);
            self.extra_energy = units(q.meal.extra);
        }
    }
}
impl World {
    pub fn uses_count_chemistry(&self) -> bool { self.environment.uses_count_chemistry() }
    pub fn set_count_chemistry(&mut self, enabled: bool) -> Result<(), String> {
        if self.uses_count_chemistry() == enabled { return Ok(()); }
        if self.ticks != 0 || !self.cells.is_empty() || !self.food.is_empty() || self.next_id != 1 {
            return Err("Choose chemical units before placing organisms or advancing the simulation.".into());
        }
        if !self.environment.set_count_chemistry(enabled) {
            return Err("The environment cannot represent those quantities as counts.".into());
        }
        Ok(())
    }
}

/// Consecutive prefix differences grant precisely the available quanta. A
/// deterministic Weyl sequence moves the rounding threshold between ticks,
/// including offers that arrive only every second or third tick.
#[derive(Default)]
struct CountQuota {
    demand: u64,
    supply: u32,
    remainder: u64,
}

impl CountQuota {
    fn new(tick: u64, patch: usize, channel: u64, demand: u64, supply: u32) -> Self {
        debug_assert!(u64::from(supply) <= demand);
        if supply == 0 || u64::from(supply) == demand {
            return Self { demand, supply, remainder: 0 };
        }
        let phase = tick.wrapping_mul(0x9e3779b97f4a7c15)
            .wrapping_add((patch as u64).wrapping_mul(0xd1b54a32d192ed03))
            .wrapping_add(channel.wrapping_mul(0x94d049bb133111eb));
        let offset = ((u128::from(phase) * u128::from(demand)) >> 64) as u64;
        Self { demand, supply, remainder: offset }
    }

    fn next(&mut self, request: u32) -> u32 {
        if request == 0 || self.supply == 0 { return 0; }
        if u64::from(self.supply) == self.demand { return request; }
        // Carry the previous remainder to produce the same prefix differences.
        // The product always fits u64. Large cross-cell demands can overflow
        // only when adding the remainder, in which case use the wider path.
        let product = u64::from(request) * u64::from(self.supply);
        if let Some(total) = product.checked_add(self.remainder) {
            let grant = total / self.demand;
            self.remainder = total - grant * self.demand;
            grant as u32
        } else {
            let total = u128::from(product) + u128::from(self.remainder);
            let demand = u128::from(self.demand);
            let grant = total / demand;
            self.remainder = (total - grant * demand) as u64;
            grant as u32
        }
    }
}

#[cfg(test)]
mod quota_tests {
    use super::CountQuota;

    #[test]
    fn rolling_grants_match_absolute_prefixes_including_wide_sums() {
        let cases = [
            (27, 7, 11, vec![2, 0, 14, 3, 8]),
            (27, 0, 0, vec![0, 22, 5]),
            (27, 27, 0, vec![7, 20]),
            (u64::MAX, u32::MAX, u64::MAX - 1, vec![u32::MAX, 1, 0, u32::MAX]),
        ];
        for (demand, supply, offset, requests) in cases {
            let mut quota = CountQuota { demand, supply, remainder: offset };
            let mut prefix = 0_u128;
            let granted = |prefix: u128| (prefix * u128::from(supply) + u128::from(offset)) / u128::from(demand);
            for request in requests {
                let previous = granted(prefix);
                prefix += u128::from(request);
                assert_eq!(u128::from(quota.next(request)), granted(prefix) - previous);
            }
        }
    }
}

impl World {
    pub(super) fn count_exchange(&mut self, ws: &mut PhaseWorkspace) {
        for oxygen in [true,false] {
            ws.clear_groups(self.environment.patch_count());
            for (i,c) in self.cells.iter_mut().enumerate() {
                if !c.alive() { continue; }
                let patch=self.environment.patch_index(c.x,c.y);
                let volume=c.gas_volume();
                let q=c.counted.as_mut().unwrap();
                let internal=if oxygen {q.oxygen} else {q.dic};
                let external=if oxygen {self.environment.oxygen_count(patch)} else {self.environment.inorganic_carbon_count(patch)};
                let exchange=(units(external)-units(internal)/volume)*(0.6*c.body.area.sqrt()*DT).min(volume*0.5);
                let outgoing=exchange<0.0;
                let index=2*usize::from(!oxygen)+usize::from(outgoing);
                let amount=rate(&mut q.rate_credit[index],exchange.abs()).min(if outgoing {internal} else {u32::MAX-internal});
                if amount!=0 { ws.group_request(patch,(i,if outgoing {-(amount as f64)} else {amount as f64})); }
            }
            for patch in occupied_patches(&ws.group_patches) {
                let group=&mut ws.groups[patch];
                group.sort_by_key(|&(i,_)|self.cells[i].id);
                let mut demand = 0_u64;
                let mut offered = 0_u64;
                for &(_, signed) in group.iter() {
                    if signed > 0.0 { demand += signed as u64; }
                    else { offered += (-signed) as u64; }
                }
                let send = offered.min(u64::from(u32::MAX)) as u32;
                let receive = demand.min(u64::from(u32::MAX)) as u32;
                let (accepted, available) = if oxygen {
                    (self.environment.release_oxygen_counts(patch, send),
                     self.environment.take_oxygen_counts(patch, receive))
                } else {
                    (self.environment.release_inorganic_carbon_counts(patch, send),
                     self.environment.take_inorganic_carbon_counts(patch, receive))
                };
                let channel = if oxygen { 1 } else { 2 };
                let mut incoming = CountQuota::new(self.ticks, patch, channel, demand, available);
                let mut outgoing = CountQuota::new(self.ticks, patch, channel + 3, offered, accepted);
                for &(i, signed) in group.iter() {
                    let q = self.cells[i].counted.as_mut().unwrap();
                    let stock = if oxygen { &mut q.oxygen } else { &mut q.dic };
                    if signed > 0.0 { *stock += incoming.next(signed as u32); }
                    else { *stock -= outgoing.next((-signed) as u32); }
                    self.cells[i].project_counts();
                }
            }
        }
    }
    pub(super) fn count_respire(&mut self, caps: &[BodyCapacities]) {
        for (c,cap) in self.cells.iter_mut().zip(caps) {
            c.respiration_rate=0.0;
            if !c.alive() {continue;}
            let q=c.counted.as_mut().unwrap();
            if q.carbon==0 {continue;}
            let basal=cap.basal_respiration.min(cap.respiration);
            let baseline_room=(cap.energy*0.25-c.energy).max(0.0);
            let density=16.0+0.8*f64::from(q.reserve_extra)/f64::from(q.carbon);
            let budget=((cap.respiration-basal)*DT+(basal*DT).min(baseline_room*16.0/density))/16.0;
            let n=rate(&mut q.rate_credit[4],budget).min(q.carbon).min(q.oxygen).min(u32::MAX-q.dic);
            let room=floor(cap.energy).saturating_sub(q.atp);
            let source=CountMeal{carbon:q.carbon,extra:q.reserve_extra,mineral:0};
            let feasible=|n| { let p=source.portion(n);16*u64::from(n)+u64::from(p.extra)*4/5<=u64::from(room) };
            let mut high=n.min(room/16);let mut low=0;
            if feasible(high) {low=high;} else {while low<high {let mid=low+(high-low).div_ceil(2);if feasible(mid){low=mid}else{high=mid-1}}}
            if low==0 {continue;}
            let part=source.portion(low);
            let yield_atp = (16*u64::from(low)+u64::from(part.extra)*4/5) as u32;
            self.energy_loss.respiration += part.energy() - u64::from(yield_atp);
            q.atp += yield_atp;
            q.carbon-=low;q.reserve_extra-=part.extra;q.oxygen-=low;q.dic+=low;
            c.respiration_rate=units(low)/DT;
            self.count_flux.respired+=u64::from(low); self.carbon_respired_total+=units(low);
            c.project_counts();
        }
    }
    pub(super) fn count_uptake(&mut self, outputs:&[Outputs], caps:&[BodyCapacities], ws:&mut PhaseWorkspace) {
        ws.clear_groups(self.environment.patch_count());
        for (i,c) in self.cells.iter_mut().enumerate() {
            if !c.alive(){continue;}
            let q=c.counted.as_mut().unwrap();
            let requested=rate(&mut q.rate_credit[5],outputs[i].absorb*caps[i].uptake*DT)
                .min(floor(caps[i].minerals).saturating_sub(q.mineral)).min(q.atp);
            if requested>0 {ws.group_request(self.environment.patch_index(c.x,c.y),(i,requested as f64));}
        }
        for patch in occupied_patches(&ws.group_patches) {
            let group=&mut ws.groups[patch];group.sort_by_key(|&(i,_)|self.cells[i].id);
            let demand:u64=group.iter().map(|&(_,n)|n as u64).sum();
            let supply=self.environment.take_mineral_counts(patch,demand.min(u32::MAX as u64) as u32);
            let mut grants = CountQuota::new(self.ticks, patch, 3, demand, supply);
            for &(i,n) in group.iter() {
                let c=&mut self.cells[i];let q=c.counted.as_mut().unwrap();
                let actual = grants.next(n as u32);
                self.energy_loss.work += u64::from(actual);
                q.mineral+=actual;q.atp-=actual;c.uptake_rate=units(actual)/DT;c.project_counts();
            }
        }
    }
    pub(super) fn count_decay(&mut self, ws:&mut PhaseWorkspace) {
        ws.clear_groups(self.environment.patch_count());
        for (i,f) in self.food.iter_mut().enumerate() {
            let q=f.counted.as_mut().unwrap();
            let requested=rate(&mut q.decay_credit,units(q.meal.carbon)*f.composition.decay_fraction()).min(q.meal.carbon);
            if requested>0 {ws.group_request(self.environment.patch_index(f.x,f.y),(i,requested as f64));}
        }
        for patch in occupied_patches(&ws.group_patches) {
            let group=&ws.groups[patch];let demand:u64=group.iter().map(|&(_,n)|n as u64).sum();
            let supply=self.environment.take_oxygen_counts(patch,(u32::MAX-self.environment.inorganic_carbon_count(patch)).min(demand.min(u32::MAX as u64) as u32));
            let mut grants = CountQuota::new(self.ticks, patch, 6, demand, supply);
            let mut used = 0;
            for &(i,n) in group {
                let f=&mut self.food[i];let q=f.counted.as_mut().unwrap();
                let request = grants.next(n as u32);
                let carbon=q.meal.fitting(request,u64::MAX,u32::MAX-self.environment.mineral_count(patch));
                let removed=q.meal.take(carbon);used+=carbon;
                self.energy_loss.decay += removed.energy();
                self.environment.release_mineral_counts(patch,removed.mineral);
                self.environment.emit_organic_patch(patch,units(carbon)*20.0*0.08);
                f.project_counts();
            }
            self.environment.release_inorganic_carbon_counts(patch,used);
            self.environment.release_oxygen_counts(patch,supply-used);
            self.count_flux.decay+=u64::from(used);self.decay_carbon_respired_total+=units(used);
        }
        self.food.retain(|f|f.counted.unwrap().meal.carbon>0);
    }
}

impl CellCounts {
    fn split_to(&mut self, daughter_struct:u32, total_struct:u32, fee:u32) -> Self {
        self.capture_credit.clear(); // Fission separates the old contact interface.
        self.atp-=fee.min(self.atp);
        fn half(n:&mut u32)->u32 {let v=*n/2;*n-=v;v}
        let mut child=Self::default();
        child.atp=half(&mut self.atp);child.mineral=half(&mut self.mineral);
        child.oxygen=half(&mut self.oxygen);child.dic=half(&mut self.dic);
        child.carbon=self.carbon/2;
        child.reserve_extra=proportional(self.reserve_extra,child.carbon,self.carbon);
        self.reserve_extra-=child.reserve_extra;self.carbon-=child.carbon;
        child.structural_extra=proportional(self.structural_extra,daughter_struct,total_struct);
        self.structural_extra-=child.structural_extra;
        for (p,d) in self.gut.iter_mut().zip(&mut child.gut){*d=p.take(p.carbon/2);}
        for (p,d) in self.rate_credit.iter_mut().zip(&mut child.rate_credit){*d=half(p);}
        child
    }
    pub(super) fn gut_room(&self, cap:&BodyCapacities)->(u64,u32) {
        let energy:u64=self.gut.iter().map(|p|p.energy()).sum();
        let mineral:u64=self.gut.iter().map(|p|u64::from(p.mineral)).sum();
        (u64::from(floor(cap.gut_energy)).saturating_sub(energy),u64::from(floor(cap.gut_minerals)).saturating_sub(mineral) as u32)
    }
}
impl Cell {
    pub(super) fn count_partition(&mut self, daughter:&mut Cell, fee:f64) {
        let d=daughter.body.count_material(); let total=self.body.count_material()+d;
        daughter.counted=Some(self.counted.as_mut().unwrap().split_to(d,total,nearest(fee)));
        self.project_counts();daughter.project_counts();
    }
    pub(super) fn count_capture(&mut self, food:&mut Food, cap:&BodyCapacities,budget:f64, port: u32, tick: u64)->f64 {
        let q=self.counted.as_mut().unwrap();let f=food.counted.as_mut().unwrap();
        let (e,m)=q.gut_room(cap);
        let mut credit = q.capture_credit.remove(&port).filter(|&(particle, last, _)|
            particle == food.id && tick.saturating_sub(last) <= 1).map_or(0, |entry| entry.2);
        let (carriers, offered) = f.meal.work_portion(&mut credit, budget, e, m);
        if credit != 0 && offered > 0.0 && carriers < f.meal.carbon {
            q.capture_credit.insert(port, (food.id, tick, credit));
        }
        let before = f.meal.carbon;
        let mut meal = f.meal.take(carriers);
        let parts = food.composition.taken(u64::from(before), u64::from(f.meal.carbon));
        for (pool, n) in q.gut.iter_mut().zip(parts) { pool.join(meal.take(n as u32)); }
        self.project_counts();
        food.project_counts();
        offered
    }
    pub(super) fn count_digest(&mut self, cap:&BodyCapacities, work:[f64;3], emissions:&mut Vec<OrganicEmission>) {
        let radius=self.radius()+2.5;
        let (x,y)=(self.x-radius*self.angle.cos(),self.y-radius*self.angle.sin());
        let q=self.counted.as_mut().unwrap();let mut assimilated=0_u64;
        for k in 0..3 {
            // Digesting a carbon costs two fifths of an ATP count. A whole
            // ATP is paid before work; carry its unused fifths to the next bite.
            let prepaid = u64::from(q.rate_credit[27]);
            let affordable = (u64::from(q.atp) * 5 + prepaid) / 2;
            let n=rate(&mut q.rate_credit[7+k],work[k]*DIGESTION_SPEED[k]/20.0)
                .min(q.gut[k].carbon).min(affordable.min(u64::from(u32::MAX)) as u32);
            if n==0 {continue;}
            let meal=q.gut[k].take(n);
            let fraction=[80_u64,65,32][k];
            let div=u64::from(n)*fraction+u64::from(q.rate_credit[10+k]);
            let room=(floor(cap.energy)/20).saturating_sub(q.carbon);
            let kept=((div/100) as u32).min(room);
            let mut accepted=meal.portion(kept);
            // Match the substrate's mineral yield even when a bite contains
            // only one mineral. Whole overflow still leaves as physical residue.
            let mineral_share = u64::from(meal.mineral)*[100,80,50][k] + u64::from(q.rate_credit[24+k]);
            accepted.mineral = ((mineral_share / 100) as u32).min(floor(cap.minerals).saturating_sub(q.mineral));
            accepted.extra=accepted.extra.min(u32::MAX-q.reserve_extra);
            // If all carbon was assimilated, any outgoing energy still needs a carrier.
            if kept==n && accepted.extra!=meal.extra {
                q.gut[k].join(meal);
                continue;
            }
            q.rate_credit[10+k] = (div % 100) as u32;
            q.rate_credit[24+k] = (mineral_share % 100) as u32;
            let due = 2 * u64::from(n);
            let paid = due.saturating_sub(prepaid).div_ceil(5);
            q.rate_credit[27] = (prepaid + paid * 5 - due) as u32;
            q.atp -= paid as u32;
            q.carbon+=kept;q.mineral+=accepted.mineral;q.reserve_extra+=accepted.extra;
            assimilated+=accepted.energy();
            let waste=CountMeal{carbon:n-kept,mineral:meal.mineral-accepted.mineral,extra:meal.extra-accepted.extra};
            if waste.carbon>0||waste.mineral>0 {
                let mut e=OrganicEmission::new(OrganicKind::Waste,x,y,wide_units(waste.energy()),units(waste.mineral)).enriched(units(waste.extra));
                e.composition=Composition::residue(k);
                e.outside=Some((self.x,self.y,radius-2.5));emissions.push(e);
            }
        }
        self.digestion_rate=wide_units(assimilated)/DT;self.project_counts();
    }
    pub(super) fn count_fix(&mut self, photons:u32,cap:&BodyCapacities)->u32 {
        let q=self.counted.as_mut().unwrap();
        let exposure = u64::from(photons) + u64::from(q.rate_credit[23]);
        q.rate_credit[23] = (exposure % CARBON_ENERGY) as u32;
        let n = ((exposure / CARBON_ENERGY) as u32).min(q.dic).min(u32::MAX - q.oxygen);
        let room=(floor(cap.energy)/20).saturating_sub(q.carbon);
        let n = n.min(room);
        q.dic -= n;
        q.oxygen += n;
        q.carbon += n;
        self.carbon_fixation_rate=units(n)/DT;self.photosynthesis_rate=units(n)*20.0/DT;
        self.project_counts();n
    }
}

#[derive(Default)]
struct PhotonShare {
    demand: u64,
    grants: CountQuota,
}

#[derive(Default)]
pub(super) struct PhotonPatches {
    shares: Vec<PhotonShare>,
    occupied: Vec<u64>,
}

impl PhotonPatches {
    fn reset(&mut self, count: usize) {
        for patch in occupied_patches(&self.occupied) { self.shares[patch].demand = 0; }
        self.shares.resize_with(count, PhotonShare::default);
        self.occupied.resize(count.div_ceil(64), 0);
        self.occupied.fill(0);
    }
}

impl World {
    /// Allocate the actual integral light falling on a patch among every cell
    /// drawing from it. A fraction below one light quantum is carried locally.
    pub(super) fn share_photons(
        &mut self,
        requests: &mut [(usize, usize, f64)],
        patches: &mut PhotonPatches,
        receipts: &mut Vec<u32>,
    ) {
        receipts.clear();
        receipts.resize(self.cells.len(), 0);
        if requests.is_empty() {
            return;
        }
        patches.reset(self.environment.patch_count());
        self.photon_credit.resize(self.environment.patch_count(), 0);
        for (row, patch, request) in requests.iter_mut() {
            let cell = self.cells[*row].counted.as_mut().unwrap();
            let wanted = rate(&mut cell.rate_credit[13], *request);
            // Every u32 is exactly representable in the shared footprint workspace.
            *request = f64::from(wanted);
            // Even a sub-quantum request samples incident light for this patch.
            patches.occupied[*patch / 64] |= 1_u64 << (*patch % 64);
            patches.shares[*patch].demand += u64::from(wanted);
        }
        for patch in occupied_patches(&patches.occupied) {
            let share = &mut patches.shares[patch];
            let incident = PHOTO_HABITAT_POWER / PATCHES as f64
                * self.environment.patch_light(patch) * DT;
            let offer = rate(&mut self.photon_credit[patch], incident);
            let available = u64::from(offer).min(share.demand) as u32;
            share.grants = CountQuota::new(self.ticks, patch, 0, share.demand, available);
        }
        for &(row, patch, wanted) in requests.iter() {
            let share = &mut patches.shares[patch];
            let grant = share.grants.next(wanted as u32);
            receipts[row] += grant;
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct LocalDeposit { pub x:f64,pub y:f64,pub quantities:[u32;3] }
pub(super) fn deposit(env:&mut Environment, pending:&mut Vec<LocalDeposit>, x:f64,y:f64, mut quantities:[u32;3]) {
    let i=env.patch_index(x,y);
    if quantities[0]>0 {quantities[0]-=env.release_mineral_counts_near(i,quantities[0]);}
    if quantities[1]>0 {quantities[1]-=env.release_oxygen_counts_near(i,quantities[1]);}
    if quantities[2]>0 {quantities[2]-=env.release_inorganic_carbon_counts_near(i,quantities[2]);}
    if quantities!=[0;3] {pending.push(LocalDeposit{x,y,quantities});}
}
impl CountMeal {
    pub(super) fn emission(self,kind:OrganicKind,x:f64,y:f64)->OrganicEmission {
        OrganicEmission::new(kind,x,y,wide_units(self.energy()),units(self.mineral)).enriched(units(self.extra))
    }
}
impl Cell {
    pub(super) fn count_overflow(&mut self,cap:&BodyCapacities,out:&mut Vec<OrganicEmission>) -> u32 {
        let q=self.counted.as_mut().unwrap();
        let mineral=q.mineral.saturating_sub(floor(cap.minerals));q.mineral-=mineral;
        let overflow = q.atp.saturating_sub(floor(cap.energy));
        q.atp -= overflow;
        let carbon=q.carbon.saturating_sub(floor(cap.energy)/20);
        let extra=proportional(q.reserve_extra,carbon,q.carbon);q.reserve_extra-=extra;q.carbon-=carbon;
        if carbon>0||mineral>0 {out.push(CountMeal{carbon,mineral,extra}.emission(OrganicKind::Exudate,self.x,self.y));}
        let mut available=u64::from(floor(cap.gut_energy));let mut minerals=floor(cap.gut_minerals);
        for (index, p) in q.gut.iter_mut().enumerate() {
            let n=p.fitting(p.carbon,available,minerals);
            let kept=p.take(n);
            if p.carbon!=0 {out.push(p.emission(OrganicKind::Waste,self.x,self.y).material(Composition::pool(index)));}
            *p=kept;available-=kept.energy();minerals-=kept.mineral;
        }
        self.project_counts();
        overflow
    }
}
/// Packet formation can combine several u32 stores at death. Split the widened
/// batch by actual carrier count before converting each physical packet to u32.
#[derive(Clone,Copy)]
struct Batch { c:u64,m:u64,e:u64 }
impl Batch {
    fn from_emission(e:&OrganicEmission)->Self {
        let counts=|n:f64|(n.max(0.0)*f64::from(counts::SCALE)).round() as u64;
        Self{c:counts((e.energy-e.extra_energy)/20.0),m:counts(e.nutrients),e:counts(e.extra_energy)}
    }
    fn portion(self,n:u64)->(u64,u64) {
        if n == 0 { return (0, 0); }
        if n == self.c { return (self.m, self.e); }
        let share = |amount: u64| match amount.checked_mul(n) {
            Some(product) => product / self.c,
            None => (u128::from(amount) * u128::from(n) / u128::from(self.c)) as u64,
        };
        (share(self.m), share(self.e))
    }
    fn take(&mut self,space:u64,minspace:u32,new_packet:bool)->CountMeal {
        let mut hi = self.c.min(u64::from(u32::MAX)).min(space / CARBON_ENERGY);
        let fits = |n| {
            let (mineral, extra) = self.portion(n);
            extra <= u64::from(u32::MAX) && mineral <= u64::from(minspace)
                && CARBON_ENERGY * n + extra <= space
        };
        let mut lo = if fits(hi) { hi } else { 0 };
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if fits(mid) { lo = mid; } else { hi = mid - 1; }
        }
        // An indivisible, unusually enriched carbon carrier may exceed the soft packet size.
        if lo==0 && new_packet && self.c>0 {lo=1;}
        let (m,e)=self.portion(lo);self.c-=lo;self.m-=m;self.e-=e;
        CountMeal{carbon:lo as u32,mineral:u32::try_from(m).expect("one carrier fits mineral inventory"),extra:u32::try_from(e).expect("one carrier fits energy inventory")}
    }
}
impl World {
    pub(super) fn count_emit(&mut self,ws:&mut PhaseWorkspace) {
        if ws.emissions.is_empty(){return;}
        let grid=&mut ws.food_grid;
        grid.configure(self.environment.width,self.environment.height);grid.clear();
        for (i,f) in self.food.iter().enumerate(){grid.insert(f.x,f.y,i);}
        let limit=u64::from(nearest(ORGANIC_PACKET_ENERGY));
        for emission in ws.emissions.drain(..) {
            let (x,y,kind)=(emission.x,emission.y,emission.kind);
            let mut batch=Batch::from_emission(&emission);
            self.environment.emit_organic(x,y,emission.energy*0.08);
            if batch.c==0 {deposit(&mut self.environment,&mut self.deposits,x,y,[batch.m as u32,0,0]);continue;}
            ws.nearby.clear();
            ws.nearby.extend(grid.nearby_bounds(x,y,emission.merge_radius).filter(|&i| {
                let food = &self.food[i];
                food.kind == kind && food.counted.as_ref().unwrap().meal.energy() + CARBON_ENERGY <= limit
                    && delta(food.x - x, self.environment.width).abs() <= emission.merge_radius
                    && delta(food.y - y, self.environment.height).abs() <= emission.merge_radius
            }));
            ws.nearby.sort_unstable();
            for &i in &ws.nearby {
                let f=&mut self.food[i];
                if delta(f.x-x,self.environment.width).hypot(delta(f.y-y,self.environment.height))>emission.merge_radius
                    ||self.environment.segment_blocked((x,y),(f.x,f.y)){continue;}
                let stock=&mut f.counted.as_mut().unwrap().meal;
                let part=batch.take(limit.saturating_sub(stock.energy()),u32::MAX-stock.mineral,false);
                if part.carbon==0 {continue;}
                let incoming = emission.composition.taken(batch.c + u64::from(part.carbon), batch.c);
                let held = f.composition.counts(u64::from(stock.carbon));
                f.composition = Composition::from_counts(std::array::from_fn(|i| held[i] + incoming[i]));
                stock.join(part);f.project_counts();
                if let Some(envelope)=emission.outside {
                    if delta(f.x-envelope.0,self.environment.width).hypot(delta(f.y-envelope.1,self.environment.height))<envelope.2 {
                        let from=(f.x,f.y);(f.x,f.y)=outside_envelope(&self.environment,(x,y),envelope,(x,y));
                        if from!=(f.x,f.y){grid.relocate(from,(f.x,f.y),i);}
                    }
                }
                if batch.c==0{break;}
            }
            let mut n=0;
            while batch.c!=0 {
                let part=batch.take(limit,u32::MAX,true);
                let angle=self.rng.unit()*TAU;let radius=if n==0{0.0}else{self.rng.unit()*emission.merge_radius.min(8.0)};
                let mut pos=(wrap(x+angle.cos()*radius,self.environment.width),wrap(y+angle.sin()*radius,self.environment.height));
                if let Some(envelope)=emission.outside {pos=outside_envelope(&self.environment,pos,envelope,(x,y));}
                let i=self.food.len();
                assert!(self.particle(kind,pos.0,pos.1,wide_units(part.energy()),units(part.mineral)),"preflighted organic storage");
                self.food[i].composition = emission.composition.portion(batch.c + u64::from(part.carbon), batch.c);
                self.food[i].counted=Some(FoodCounts{meal:part,decay_credit:0});
                self.food[i].project_counts();grid.insert(self.food[i].x,self.food[i].y,i);n+=1;
            }
        }
    }
}

#[derive(Default,Clone,Copy,Debug)]
pub(super) struct Flux{
    pub mineral_in:u64,pub carbon_in:u64,pub energy_in:u64,pub energy_out:u64,pub mineral_out:u64,
    pub fixed:u64,pub respired:u64,pub decay:u64,
}
impl World {
    pub(super) fn project_flows(&mut self){
        let q=self.count_flux;
        self.nutrient_imports=wide_units(q.mineral_in);self.carbon_imports=wide_units(q.carbon_in);
        self.energy_imports=wide_units(q.energy_in);self.energy_withdrawn=wide_units(q.energy_out);self.mineral_withdrawn=wide_units(q.mineral_out);
        self.carbon_fixed_total=wide_units(q.fixed);self.carbon_respired_total=wide_units(q.respired);self.decay_carbon_respired_total=wide_units(q.decay);
        self.photosynthesis_total=wide_units(q.fixed)*20.0;
    }
    /// [mineral held, imported, removed; carbon held, imported, removed;
    /// O2 held, imported+fixed, removed+respired+decayed; energy held, supplied, withdrawn].
    pub fn count_audit(&self)->Option<[u64;12]> {
        if !self.uses_count_chemistry(){return None;}
        let a=self.count_flux;let ext=self.environment.external_input_counts();let out=self.environment.external_removed_counts();
        let [mut mineral,mut oxygen,mut carbon]=self.environment.dissolved_counts();let mut energy=0u64;
        for d in &self.deposits {mineral+=u64::from(d.quantities[0]);oxygen+=u64::from(d.quantities[1]);carbon+=u64::from(d.quantities[2]);}
        for c in &self.cells {
            let q=c.counted.as_ref()?;let body=u64::from(c.body.count_material());let inv=u64::from(c.body.count_investment());
            mineral+=body+u64::from(q.mineral);carbon+=body+u64::from(q.carbon)+u64::from(q.dic);oxygen+=u64::from(q.oxygen);
            energy+=20*(body+u64::from(q.carbon))+inv+u64::from(q.atp)+u64::from(q.reserve_extra)+u64::from(q.structural_extra);
            for p in q.gut {mineral+=u64::from(p.mineral);carbon+=u64::from(p.carbon);energy+=p.energy();}
        }
        for f in &self.food{let p=f.counted?.meal;mineral+=u64::from(p.mineral);carbon+=u64::from(p.carbon);energy+=p.energy();}
        Some([mineral,ext[0]+a.mineral_in,out[0]+a.mineral_out,carbon,ext[2]+a.carbon_in,out[2],
            oxygen,ext[1]+a.fixed,out[1]+a.respired+a.decay,energy,a.energy_in+a.fixed*20,a.energy_out])
    }
}

#[cfg(test)]
mod packet_fit_tests {
    use super::*;

    #[test]
    fn counted_packet_fit_matches_wide_arithmetic_and_preserves_remainder() {
        let max = u64::from(u32::MAX);
        for batch in [Batch { c: 0, m: 0, e: 0 },
            Batch { c: 10, m: 2, e: 0 }, Batch { c: 1, m: 13, e: 50 },
            Batch { c: max + 300, m: max + 700, e: 3 * max }] {
            for space in [0, 19, 20, 69, 70, 830, 24 * max, u64::MAX] {
                for minspace in [0, 2, u32::MAX] {
                    for new_packet in [false, true] {
                        let mut actual = batch;
                        let taken = actual.take(space, minspace, new_packet);
                        let share = |amount, n| if batch.c == 0 { 0 } else {
                            (u128::from(amount) * u128::from(n) / u128::from(batch.c)) as u64
                        };
                        let mut low = 0;
                        let mut high = batch.c.min(max);
                        while low < high {
                            let n = low + (high - low).div_ceil(2);
                            let (m, e) = (share(batch.m, n), share(batch.e, n));
                            if 20 * n + e <= space && m <= u64::from(minspace) && e <= max {
                                low = n;
                            } else { high = n - 1; }
                        }
                        let n = if new_packet && low == 0 && batch.c > 0 { 1 } else { low };
                        let (m, e) = (share(batch.m, n), share(batch.e, n));
                        assert_eq!((u64::from(taken.carbon), u64::from(taken.mineral), u64::from(taken.extra)), (n, m, e));
                        assert_eq!((actual.c, actual.m, actual.e), (batch.c - n, batch.m - m, batch.e - e));
                    }
                }
            }
        }
    }
}
