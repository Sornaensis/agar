//! A counted cell pays for a whole material or ATP unit before using a
//! fraction of it for continuous work. The remaining credit belongs to the body.
use super::Body;
use crate::checkpoint::{ensure, Reader, Result, Writer};
use crate::counts::{nearest, rate, units, SCALE};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyFunding {
    pub(crate) material: u32,
    pub(crate) investment: u32,
    pub(crate) material_credit: f64,
    pub(crate) energy_credit: f64,
    retained_fraction: f64,
    // Fractions of unfinished molecular operations, never spendable inventory.
    clocks: BTreeMap<u64, u32>,
}

impl BodyFunding {
    pub(crate) fn complete_work(&mut self, key: u64, amount: f64) -> u32 {
        let credit = self.clocks.entry(key).or_default();
        rate(credit, amount)
    }

    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self> {
        let mut value = Self {
            material: r.u32()?,
            investment: r.u32()?,
            material_credit: r.range(0.0, 1.0)?,
            energy_credit: r.range(0.0, 1.0)?,
            retained_fraction: r.range(0.0, 1.0)?,
            clocks: BTreeMap::new(),
        };
        let n = r.count(1 << 20)?;
        ensure(n <= r.remaining() / 12, "truncated developmental clocks")?;
        for _ in 0..n {
            let key = r.u64()?;
            ensure(value.clocks.insert(key, r.u32()?).is_none(), "duplicate developmental clock")?;
        }
        Ok(value)
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.u32(self.material);
        w.u32(self.investment);
        for v in [self.material_credit, self.energy_credit, self.retained_fraction] {
            w.f64(v);
        }
        w.count(self.clocks.len());
        for (&key, &phase) in &self.clocks {
            w.u64(key);
            w.u32(phase);
        }
    }

    /// A negative charge returns previously purchased units on disassembly.
    pub(crate) fn charge(credit: &mut f64, amount: f64) -> i64 {
        let due = amount * f64::from(SCALE) - *credit;
        let near = due.round();
        let counts = if (due - near).abs() < 1e-8 { near } else { due.ceil() };
        *credit = (counts - due).clamp(0.0, 1.0);
        counts as i64
    }

    pub(crate) fn settle_investment(&mut self, change: f64) {
        let accrued = change * f64::from(SCALE) + self.retained_fraction;
        let near = accrued.round();
        let n = if (accrued - near).abs() < 1e-8 { near } else { accrued.floor() };
        self.retained_fraction = (accrued - n).clamp(0.0, 1.0);
        self.investment = (i64::from(self.investment) + n as i64).clamp(0, i64::from(u32::MAX)) as u32;
    }
}

impl Body {
    pub(crate) fn prepare_count_founder(&mut self) {
        self.count_funding = Some(BodyFunding {
            material: nearest(self.structural_mass()),
            investment: nearest(self.embodied_energy()),
            ..Default::default()
        });
    }

    pub fn has_count_funding(&self) -> bool { self.count_funding.is_some() }

    pub(crate) fn count_material(&self) -> u32 {
        self.count_funding.as_ref().map_or_else(|| nearest(self.structural_mass()), |b| b.material)
    }

    pub(crate) fn count_investment(&self) -> u32 {
        self.count_funding.as_ref().map_or_else(|| nearest(self.embodied_energy()), |b| b.investment)
    }

    pub(super) fn partition_counts(&self, left: &mut Self, right: &mut Self) {
        let Some(paid) = &self.count_funding else { return };
        // A molecule carries its actual embodied energy. Whole installed
        // machines have already been assigned to one daughter by the body.
        if let (Some(old), Some(a), Some(b)) = (&self.cellular, &mut left.cellular, &mut right.cellular) {
            for ((stock, x), y) in old.pools.iter().zip(&mut a.pools).zip(&mut b.pools) {
                let n = nearest(stock.mass);
                let k = n / 2;
                x.mass = units(k);
                y.mass = units(n - k);
                x.embodied_energy = if n == 0 { 0.0 } else { stock.embodied_energy * (f64::from(k) / f64::from(n)) };
                y.embodied_energy = stock.embodied_energy - x.embodied_energy;
            }
        }
        fn split(n: u32, share: f64, whole: f64) -> u32 {
            if whole <= 0.0 { 0 } else { (f64::from(n) * (share / whole).clamp(0.0, 1.0)).round() as u32 }
        }
        let material = split(paid.material, left.structural_mass(), self.structural_mass());
        let investment = split(paid.investment, left.embodied_energy(), self.embodied_energy());
        let mut a = paid.clone();
        a.material = material;
        a.investment = investment;
        left.count_funding = Some(a);
        right.count_funding = Some(BodyFunding {
            material: paid.material - material,
            investment: paid.investment - investment,
            ..Default::default()
        });
    }

    pub(crate) fn validate_count_funding(&self) -> Result<()> {
        let funding = self.count_funding.as_ref().ok_or("Missing construction accounts")?;
        let close = |raw: u32, progress: f64| (f64::from(raw) - progress * f64::from(SCALE)).abs() <= 4.0;
        ensure(
            close(funding.material, self.structural_mass()) && close(funding.investment, self.embodied_energy()),
            "construction accounts disagree with their material or recorded work",
        )?;
        if let Some(state) = &self.cellular {
            let whole = |mass: f64| units(nearest(mass)) == mass;
            ensure(
                state.pools.iter().all(|p| whole(p.mass))
                && state.assemblies.iter().all(|a| a.ingredients.iter().all(|p| whole(p.mass))),
                "fractional product inventory in a counted body",
            )?;
        }
        Ok(())
    }
}
