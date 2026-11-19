use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};

impl Environment {
    pub(crate) fn write_checkpoint(&self, w: &mut Writer) {
        w.f64(self.width);
        w.f64(self.height);
        for value in [
            self.ambient_temperature,
            self.ambient_light,
            self.current_strength,
            self.initial_nutrients,
            self.nutrient_inputs,
            self.nutrient_removed,
        ] {
            w.f64(value);
        }
        w.u32(self.next_barrier_id);
        w.u32(self.next_source_id);
        w.count(self.sources.len());
        for source in &self.sources {
            w.u32(source.id);
            w.u32(source.kind);
            for value in [
                source.x,
                source.y,
                source.strength,
                source.radius,
                source.angle,
                source.length,
            ] {
                w.f64(value);
            }
        }
        w.count(self.barriers.len());
        for barrier in &self.barriers {
            w.u32(barrier.id);
            w.u8(u8::from(barrier.floating));
            for value in [
                barrier.x,
                barrier.y,
                barrier.length,
                barrier.width,
                barrier.angle,
                barrier.vx,
                barrier.vy,
            ] {
                w.f64(value);
            }
        }
        // Retain derived scalar fields to avoid platform-dependent source recomputation.
        for field in [
            &self.temperature,
            &self.light,
            &self.heating,
            &self.nutrients,
            &self.nutrient_injection,
            &self.organic,
            &self.nutrient_absorption,
        ] {
            w.count(field.len());
            for value in field {
                w.f64(*value);
            }
        }
        self.gases.write_checkpoint(w);
    }
    pub(crate) fn read_checkpoint(r: &mut Reader<'_>, version: u32) -> Result<Self> {
        let (width, height) = if version >= 8 {
            (r.range(400.0, 4800.0)?, r.range(400.0, 3040.0)?)
        } else {
            (WIDTH, HEIGHT)
        };
        ensure(
            width % PATCH_SIZE == 0.0 && height % PATCH_SIZE == 0.0,
            "invalid world dimensions",
        )?;
        let columns = (width / PATCH_SIZE) as usize;
        let rows = (height / PATCH_SIZE) as usize;
        let patches = columns * rows;
        let ambient_temperature = r.range(0.0, 1.0)?;
        let ambient_light = r.range(0.0, 1.0)?;
        let current_strength = r.range(0.0, 30.0)?;
        let initial_nutrients = r.range(0.0, 5.0)?;
        let nutrient_inputs = r.range(0.0, 1e100)?;
        let nutrient_removed = if version >= 8 {
            r.range(0.0, 1e100)?
        } else {
            0.0
        };
        let next_barrier_id = if version >= 8 { r.u32()? } else { 1 };
        let next_source_id = r.u32()?;
        ensure(
            next_source_id > 0 && next_barrier_id > 0,
            "invalid environment identifier sequence",
        )?;
        let count = r.count(MAX_SOURCES)?;
        let mut sources = Vec::with_capacity(count);
        let mut ids = std::collections::HashSet::new();
        for _ in 0..count {
            let id = r.u32()?;
            let kind = r.u32()?;
            ensure(
                id > 0
                    && id < next_source_id
                    && ids.insert(id)
                    && kind
                        <= if version >= 9 {
                            12
                        } else if version >= 8 {
                            7
                        } else {
                            3
                        },
                "invalid environment source",
            )?;
            let x = r.range(0.0, width)?;
            let y = r.range(0.0, height)?;
            ensure(x < width && y < height, "source is outside the habitat")?;
            let strength = r.range(
                if matches!(kind, 0 | 3 | 4 | 8..=12) {
                    0.0
                } else {
                    -1.0
                },
                1.0,
            )?;
            let radius = r.range(20.0, 1000.0)?;
            let angle = if version >= 8 {
                r.range(0.0, TAU)?
            } else {
                0.0
            };
            let length = if version >= 8 {
                r.range(40.0, 4800.0)?
            } else {
                600.0
            };
            sources.push(Source {
                id,
                kind,
                x,
                y,
                strength,
                radius,
                angle,
                length,
            });
        }
        let count = if version >= 8 {
            r.count(MAX_BARRIERS)?
        } else {
            0
        };
        let mut barriers = Vec::with_capacity(count);
        ids.clear();
        for _ in 0..count {
            let id = r.u32()?;
            let floating = r.u8()?;
            ensure(
                id > 0 && id < next_barrier_id && ids.insert(id) && floating <= 1,
                "invalid environment barrier",
            )?;
            let x = r.range(0.0, width)?;
            let y = r.range(0.0, height)?;
            ensure(x < width && y < height, "barrier is outside the habitat")?;
            barriers.push(Barrier {
                id,
                x,
                y,
                floating: floating == 1,
                length: r.range(20.0, 2400.0)?,
                width: r.range(10.0, 400.0)?,
                angle: r.range(0.0, TAU)?,
                vx: r.range(-60.0, 60.0)?,
                vy: r.range(-60.0, 60.0)?,
            });
        }
        fn field(r: &mut Reader<'_>, count: usize, low: f64, high: f64) -> Result<Vec<f64>> {
            ensure(r.count(count)? == count, "incorrect environment grid size")?;
            (0..count).map(|_| r.range(low, high)).collect()
        }
        ensure(
            barriers
                .iter()
                .all(|b| super::geometry::barrier_fits(b.length, b.width, b.angle, width, height)),
            "barrier exceeds world extent",
        )?;
        let mut result = Self {
            width,
            height,
            columns,
            rows,
            ambient_temperature,
            ambient_light,
            current_strength,
            initial_nutrients,
            sources,
            barriers,
            blocked_right: vec![false; patches],
            blocked_down: vec![false; patches],
            solid_patches: vec![false; patches],
            flow: FlowCache::new(patches),
            barrier_cache: BarrierCache::new(patches),
            grid: PatchGeometry::new(width, height, columns, rows),
            temperature: field(r, patches, 0.0, 1.0)?,
            light: field(r, patches, 0.0, 1.0)?,
            heating: field(r, patches, -2.0, 2.0)?,
            nutrients: field(r, patches, 0.0, 1e100)?,
            nutrient_injection: field(r, patches, 0.0, 96.0)?,
            organic: field(r, patches, 0.0, 1e6)?,
            nutrient_absorption: if version >= 8 {
                field(r, patches, 0.0, 96.0)?
            } else {
                vec![0.0; patches]
            },
            nutrient_inputs,
            nutrient_removed,
            next_source_id,
            next_barrier_id,
            scratch: FieldScratch::default(),
            gases: if version >= 9 {
                GasState::read_checkpoint(r, patches)?
            } else {
                GasState::new(patches)
            },
        };
        result.rebuild_flow_samples();
        result.rebuild_gas_sources();
        result.rebuild_barriers();
        Ok(result)
    }
}
