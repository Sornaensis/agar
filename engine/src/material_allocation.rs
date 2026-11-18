//! Progressive material allocation with sparse product costs and reusable work
//! buffers. A request uses five common resources and, for installation, at most
//! one existing product pool. Product synthesis never augments this snapshot.

const COMMON_RESOURCES: usize = 5;
const EPS: f64 = 1e-12;

#[derive(Clone, Copy, Debug)]
pub(crate) struct MaterialDemand<T> {
    pub operation: T,
    pub amount: f64,
    /// Energy, minerals, internal space, surface space, translation/assembly work.
    pub common: [f64; COMMON_RESOURCES],
    /// One unit of this existing product stock per unit installed material.
    pub stock: Option<usize>,
}

impl<T> MaterialDemand<T> {
    pub fn new(operation: T, amount: f64, common: [f64; COMMON_RESOURCES]) -> Self {
        Self {
            operation,
            amount,
            common,
            stock: None,
        }
    }
}

/// Transient workspace, shared sequentially by cells and omitted from saves.
/// Once capacities are sufficient, allocation performs no heap allocations.
#[derive(Default, Debug)]
pub(crate) struct AllocationScratch {
    resources: Vec<f64>,
    needed: Vec<f64>,
    result: Vec<f64>,
    active: Vec<bool>,
}

impl AllocationScratch {
    pub fn allocate<T>(
        &mut self,
        demands: &[MaterialDemand<T>],
        common: [f64; COMMON_RESOURCES],
        stocks: &[f64],
    ) -> &[f64] {
        let resource_count = COMMON_RESOURCES + stocks.len();
        self.resources.clear();
        self.resources.extend_from_slice(&common);
        self.resources.extend_from_slice(stocks);
        self.needed.resize(resource_count, 0.0);
        self.result.resize(demands.len(), 0.0);
        self.result.fill(0.0);
        self.active.resize(demands.len(), true);
        self.active.fill(true);
        for _ in 0..=resource_count {
            self.needed.fill(0.0);
            for (index, demand) in demands.iter().enumerate() {
                if self.active[index] {
                    let remaining = demand.amount - self.result[index];
                    // Keep demand order and common-resource order identical to
                    // the dense allocator, including floating-point summation.
                    for (r, cost) in demand.common.iter().enumerate() {
                        self.needed[r] += remaining * cost;
                    }
                    if let Some(stock) = demand.stock {
                        self.needed[COMMON_RESOURCES + stock] += remaining;
                    }
                }
            }
            if self.needed.iter().all(|&value| value <= 0.0) {
                break;
            }
            let scale = self
                .needed
                .iter()
                .enumerate()
                .filter(|(_, n)| **n > 0.0)
                .map(|(r, &n)| self.resources[r] / n)
                .fold(1.0, f64::min)
                .clamp(0.0, 1.0);
            for (index, demand) in demands.iter().enumerate() {
                if self.active[index] {
                    let added = (demand.amount - self.result[index]) * scale;
                    self.result[index] += added;
                    for (r, cost) in demand.common.iter().enumerate() {
                        self.resources[r] = (self.resources[r] - added * cost).max(0.0);
                    }
                    if let Some(stock) = demand.stock {
                        let resource = &mut self.resources[COMMON_RESOURCES + stock];
                        *resource = (*resource - added).max(0.0);
                    }
                }
            }
            if scale >= 1.0 - f64::EPSILON {
                break;
            }
            let mut froze = false;
            for (index, demand) in demands.iter().enumerate() {
                if !self.active[index] {
                    continue;
                }
                let blocked = demand.common.iter().enumerate().any(|(r, &cost)| {
                    self.needed[r] > 0.0 && self.resources[r] <= EPS && cost > 0.0
                }) || demand.stock.is_some_and(|stock| {
                    let r = COMMON_RESOURCES + stock;
                    self.needed[r] > 0.0 && self.resources[r] <= EPS
                });
                if blocked {
                    self.active[index] = false;
                    froze = true;
                }
            }
            if !froze {
                break;
            }
        }
        &self.result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Kept only as an independent regression oracle for the previous dense
    // traversal. Production never creates per-demand vectors of stock costs.
    fn dense_reference(
        demands: &[MaterialDemand<()>],
        common: [f64; 5],
        stocks: &[f64],
    ) -> Vec<f64> {
        let mut resources = common.to_vec();
        resources.extend_from_slice(stocks);
        let costs: Vec<Vec<f64>> = demands
            .iter()
            .map(|demand| {
                let mut costs = vec![0.0; resources.len()];
                costs[..5].copy_from_slice(&demand.common);
                if let Some(stock) = demand.stock {
                    costs[5 + stock] = 1.0;
                }
                costs
            })
            .collect();
        let mut result = vec![0.0; demands.len()];
        let mut active = vec![true; demands.len()];
        for _ in 0..=resources.len() {
            let mut needed = vec![0.0; resources.len()];
            for (index, demand) in demands.iter().enumerate() {
                if active[index] {
                    for (r, value) in needed.iter_mut().enumerate() {
                        *value += (demand.amount - result[index]) * costs[index][r];
                    }
                }
            }
            if needed.iter().all(|&value| value <= 0.0) {
                break;
            }
            let scale = needed
                .iter()
                .enumerate()
                .filter(|(_, n)| **n > 0.0)
                .map(|(r, &n)| resources[r] / n)
                .fold(1.0, f64::min)
                .clamp(0.0, 1.0);
            for (index, demand) in demands.iter().enumerate() {
                if active[index] {
                    let added = (demand.amount - result[index]) * scale;
                    result[index] += added;
                    for (r, resource) in resources.iter_mut().enumerate() {
                        *resource = (*resource - added * costs[index][r]).max(0.0);
                    }
                }
            }
            if scale >= 1.0 - f64::EPSILON {
                break;
            }
            let mut froze = false;
            for (index, _) in demands.iter().enumerate() {
                if active[index]
                    && (0..resources.len())
                        .any(|r| needed[r] > 0.0 && resources[r] <= EPS && costs[index][r] > 0.0)
                {
                    active[index] = false;
                    froze = true;
                }
            }
            if !froze {
                break;
            }
        }
        result
    }

    #[test]
    fn missing_ingredient_stops_only_its_consumers() {
        let mut scratch = AllocationScratch::default();
        let mut a = MaterialDemand::new((), 2.0, [1.0, 0.0, 0.0, 0.0, 1.0]);
        a.stock = Some(0);
        let b = MaterialDemand::new((), 2.0, [1.0, 1.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            scratch.allocate(&[a, b], [2.0, 2.0, 0.0, 0.0, 2.0], &[0.0]),
            &[0.0, 2.0]
        );
    }

    #[test]
    fn competitors_share_completion_fractions_without_creating_material() {
        let mut scratch = AllocationScratch::default();
        let mut a = MaterialDemand::new((), 1.0, [2.0, 0.0, 1.0, 1.0, 1.0]);
        a.stock = Some(0);
        let mut b = a;
        b.amount = 3.0;
        let result = scratch.allocate(&[a, b], [100.0; 5], &[2.0]);
        assert_eq!(result, &[0.5, 1.5]);
        assert_eq!(result.iter().sum::<f64>(), 2.0);
    }

    #[test]
    fn sparse_filling_is_bit_exact_to_dense_across_resource_bottlenecks() {
        let mut rng = 0x610ef57da34a81c3_u64;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 11) as f64 / ((1_u64 << 53) as f64)
        };
        let mut scratch = AllocationScratch::default();
        for pools in [0, 1, 16, 64, 257, 1024] {
            for case in 0..32 {
                let common =
                    std::array::from_fn(|_| if next() < 0.2 { 0.0 } else { next() * 20.0 });
                let stocks: Vec<_> = (0..pools)
                    .map(|_| if next() < 0.2 { 0.0 } else { next() })
                    .collect();
                let demands: Vec<_> = (0..case * 3)
                    .map(|_| {
                        let mut demand = MaterialDemand::new(
                            (),
                            next() * 5.0,
                            std::array::from_fn(|_| if next() < 0.3 { 0.0 } else { next() * 10.0 }),
                        );
                        if pools > 0 && next() < 0.7 {
                            demand.stock = Some((next() * pools as f64) as usize);
                        }
                        demand
                    })
                    .collect();
                let expected = dense_reference(&demands, common, &stocks);
                let actual = scratch.allocate(&demands, common, &stocks);
                assert_eq!(
                    actual.iter().map(|n| n.to_bits()).collect::<Vec<_>>(),
                    expected.iter().map(|n| n.to_bits()).collect::<Vec<_>>(),
                    "pools={pools}, case={case}"
                );
                for r in 0..5 {
                    let spent: f64 = demands
                        .iter()
                        .zip(actual)
                        .map(|(d, a)| d.common[r] * a)
                        .sum();
                    assert!(spent <= common[r] + 1e-9);
                }
                for (pool, stock) in stocks.iter().enumerate() {
                    let spent: f64 = demands
                        .iter()
                        .zip(actual)
                        .filter(|(d, _)| d.stock == Some(pool))
                        .map(|(_, a)| a)
                        .sum();
                    assert!(spent <= stock + 1e-9);
                }
            }
        }
    }

    #[test]
    fn scratch_reuses_buffers_and_clears_between_different_cells() {
        let mut scratch = AllocationScratch::default();
        let mut a = MaterialDemand::new((), 2.0, [1.0; 5]);
        a.stock = Some(63);
        scratch.allocate(&[a; 100], [100.0; 5], &[1.0; 64]);
        let pointers = (
            scratch.resources.as_ptr(),
            scratch.needed.as_ptr(),
            scratch.result.as_ptr(),
            scratch.active.as_ptr(),
        );
        let b = MaterialDemand::new((), 0.5, [1.0; 5]);
        for _ in 0..20 {
            assert_eq!(scratch.allocate(&[b], [1.0; 5], &[]), &[0.5]);
            assert_eq!(scratch.allocate::<()>(&[], [0.0; 5], &[0.0; 64]), &[]);
            assert_eq!(
                pointers,
                (
                    scratch.resources.as_ptr(),
                    scratch.needed.as_ptr(),
                    scratch.result.as_ptr(),
                    scratch.active.as_ptr()
                )
            );
        }
    }
}
