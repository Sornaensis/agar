//! Integer inventories use the same scale for mass, chemical amount and ATP.
//! Rates and geometry can be real; an inventory changes only by whole counts.
pub const SCALE: u32 = 10_000;
pub const CARBON_ENERGY: u64 = 20;

pub fn units(n: u32) -> f64 { n as f64 / SCALE as f64 }
pub fn wide_units(n: u64) -> f64 { n as f64 / SCALE as f64 }
pub fn floor(x: f64) -> u32 {
    let n = x.max(0.0) * f64::from(SCALE);
    // Canonical projected counts can multiply back just below their integer.
    (n + n.abs() * f64::EPSILON).floor() as u32
}
pub fn nearest(x: f64) -> u32 { (x.max(0.0) * f64::from(SCALE)).round() as u32 }
pub fn proportional(quantity: u32, part: u32, whole: u32) -> u32 {
    if whole == 0 { 0 } else { (u64::from(quantity) * u64::from(part.min(whole)) / u64::from(whole)) as u32 }
}
/// The fractional credit is one binary fraction of a count. Blocked whole
/// opportunities expire, so a dormant organ cannot bank unlimited future work.
pub fn rate(credit: &mut u32, amount: f64) -> u32 {
    if !(amount > 0.0) { return 0; }
    let counts = (amount * f64::from(SCALE)).min(f64::from(u32::MAX));
    let whole = counts.floor() as u32;
    let frac = ((counts.fract() * 4294967296.0).floor() as u64) + u64::from(*credit);
    *credit = frac as u32;
    whole.saturating_add((frac >> 32) as u32)
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct CountMeal { pub carbon: u32, pub mineral: u32, pub extra: u32 }
impl CountMeal {
    pub fn energy(self) -> u64 { CARBON_ENERGY * u64::from(self.carbon) + u64::from(self.extra) }
    pub fn portion(self, carbon: u32) -> Self {
        let carbon = carbon.min(self.carbon);
        Self { carbon, mineral: proportional(self.mineral, carbon, self.carbon), extra: proportional(self.extra, carbon, self.carbon) }
    }
    pub fn take(&mut self, carbon: u32) -> Self {
        let part = self.portion(carbon);
        self.carbon -= part.carbon;
        self.mineral -= part.mineral;
        self.extra -= part.extra;
        part
    }
    pub fn join(&mut self, part: Self) {
        self.carbon = self.carbon.checked_add(part.carbon).expect("planned carbon headroom");
        self.mineral = self.mineral.checked_add(part.mineral).expect("planned mineral headroom");
        self.extra = self.extra.checked_add(part.extra).expect("planned chemical energy headroom");
    }
    /// Charge at most the offered work once. Work too small to move one
    /// carrier persists as a fraction of that operation for subsequent contacts.
    pub fn work_portion(self, credit: &mut u32, budget: f64, energy_room: u64, mineral_room: u32) -> (u32, f64) {
        let possible = self.fitting(self.carbon, energy_room, mineral_room);
        if possible == 0 || budget <= 0.0 { return (0, 0.0); }
        let all = wide_units(self.portion(possible).energy());
        if budget >= all { return (possible, all); }
        let work = budget * f64::from(self.carbon) / self.energy() as f64;
        (rate(credit, work).min(possible), budget)
    }
    pub fn fitting(self, max_carbon: u32, energy_room: u64, mineral_room: u32) -> u32 {
        let mut high = max_carbon.min(self.carbon);
        let fits = |n| { let p = self.portion(n); p.energy() <= energy_room && p.mineral <= mineral_room };
        if fits(high) { return high; }
        let mut low = 0;
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if fits(mid) { low = mid; } else { high = mid - 1; }
        }
        low
    }
}
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct CellCounts {
    pub atp: u32, pub mineral: u32, pub carbon: u32, pub oxygen: u32, pub dic: u32,
    pub reserve_extra: u32, pub structural_extra: u32,
    pub gut: [CountMeal; 3],
    pub rate_credit: [u32; 32],
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FoodCounts { pub meal: CountMeal, pub decay_credit: u32 }

use crate::checkpoint::{Reader,Writer,Result,ensure};
impl CountMeal {
    pub(crate) fn write(self,w:&mut Writer){for n in [self.carbon,self.mineral,self.extra]{w.u32(n);}}
    pub(crate) fn read(r:&mut Reader<'_>)->Result<Self>{
        let p=Self{carbon:r.u32()?,mineral:r.u32()?,extra:r.u32()?};
        ensure(p.carbon!=0 || (p.extra==0 && p.mineral==0),"food mineral or extra energy without a carbon carrier")?;Ok(p)
    }
}
impl CellCounts {
    pub(crate) fn write(&self,w:&mut Writer) {
        for n in [self.atp,self.mineral,self.carbon,self.oxygen,self.dic,self.reserve_extra,self.structural_extra]{w.u32(n);}
        for p in self.gut{p.write(w);} for n in self.rate_credit{w.u32(n);}
    }
    pub(crate) fn read(r:&mut Reader<'_>)->Result<Self>{
        let mut s=Self{atp:r.u32()?,mineral:r.u32()?,carbon:r.u32()?,oxygen:r.u32()?,dic:r.u32()?,
            reserve_extra:r.u32()?,structural_extra:r.u32()?,..Default::default()};
        ensure(s.carbon>0||s.reserve_extra==0,"extra energy without counted reserve carbon")?;
        for p in &mut s.gut{*p=CountMeal::read(r)?;}for n in &mut s.rate_credit{*n=r.u32()?;}
        ensure(s.rate_credit[10..13].iter().all(|&n|n<100)&&s.rate_credit[14]<25 && s.rate_credit[23]<20,"invalid whole-count reaction remainder")?;Ok(s)
    }
}
