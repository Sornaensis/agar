use super::*;
use crate::body::BodyFunding;
use crate::counts::{proportional, units, SCALE};

impl Cell {
    pub(super) fn develop_counted(
        &mut self,
        program: &Program,
        output: &Outputs,
        cap: &BodyCapacities,
        geometry_ready: bool,
        scratch: &mut CellularWorkScratch,
    ) -> crate::body::WorkReport {
        let prior_mass = self.body.structural_mass();
        let prior_investment = self.body.embodied_energy();
        let funds = self.body.count_funding.as_ref().expect("counted founder body");
        let counts = self.counted.as_mut().expect("counted founder stores");
        let mut available = counts.mineral.min(counts.carbon);
        let headroom = u32::MAX - counts.structural_extra;
        if counts.reserve_extra > headroom {
            // floor(extra * amount / carbon) must fit in the structure.
            let allowed = ((u64::from(headroom) + 1) * u64::from(counts.carbon) - 1)
                / u64::from(counts.reserve_extra);
            available = available.min(allowed as u32);
        }
        let mut material = units(available) + funds.material_credit / f64::from(SCALE);
        let mut energy = units(counts.atp) + funds.energy_credit / f64::from(SCALE);
        let starting_energy = energy;
        let reserve = if program.cellular().is_some() { 0.0 } else { 0.15 * cap.energy };
        let repaired = (cap.repair * output.repair * DT)
            .min(1.0 - self.integrity)
            .min((energy - reserve).max(0.0) / (20.0 * self.body.area));
        energy -= repaired * 20.0 * self.body.area;
        self.integrity += repaired;

        let report = if let (Some(def), Some(controls)) = (program.cellular(), &output.cellular) {
            self.body.cellular_work_in(
                def, controls, &mut energy, &mut material, DT, MAX_AREA, geometry_ready, scratch,
            )
        } else {
            self.body.work(
                output.grow, &output.body, output.repair, &mut energy, &mut material, DT, MAX_AREA,
            )
        };

        let mass_change = self.body.structural_mass() - prior_mass;
        let bound_change = self.body.embodied_energy() - prior_investment;
        let funds = self.body.count_funding.as_mut().unwrap();
        let charge = BodyFunding::charge(&mut funds.energy_credit, starting_energy - energy);
        counts.atp = (i64::from(counts.atp) - charge).try_into().expect("funded ATP");
        let mass = BodyFunding::charge(&mut funds.material_credit, mass_change);
        if mass > 0 {
            let n = mass as u32;
            let extra = proportional(counts.reserve_extra, n, counts.carbon);
            counts.reserve_extra -= extra;
            counts.structural_extra += extra;
            counts.carbon -= n;
            counts.mineral -= n;
            funds.material += n;
        } else if mass < 0 {
            let n = (-mass) as u32;
            // A full reserve leaves excess bound energy on the remaining frame.
            let extra = proportional(counts.structural_extra, n, funds.material)
                .min(u32::MAX - counts.reserve_extra);
            counts.structural_extra -= extra;
            counts.reserve_extra += extra;
            counts.carbon += n;
            counts.mineral += n;
            funds.material -= n;
        }
        funds.settle_investment(bound_change);
        self.project_counts();
        report
    }
}
