use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};

impl Environment {
    pub(crate) fn write_checkpoint(&self, w: &mut Writer) {
        for value in [
            self.ambient_temperature,
            self.ambient_light,
            self.current_strength,
            self.initial_nutrients,
            self.nutrient_inputs,
        ] {
            w.f64(value);
        }
        w.u32(self.next_source_id);
        w.count(self.sources.len());
        for source in &self.sources {
            w.u32(source.id);
            w.u32(source.kind);
            for value in [source.x, source.y, source.strength, source.radius] {
                w.f64(value);
            }
        }
        // Retaining derived fields also avoids platform-dependent recomputation on load.
        for field in [
            &self.temperature,
            &self.light,
            &self.heating,
            &self.nutrients,
            &self.nutrient_injection,
            &self.organic,
        ] {
            w.count(field.len());
            for value in field {
                w.f64(*value);
            }
        }
    }
    pub(crate) fn read_checkpoint(r: &mut Reader<'_>) -> Result<Self> {
        let ambient_temperature = r.range(0.0, 1.0)?;
        let ambient_light = r.range(0.0, 1.0)?;
        let current_strength = r.range(0.0, 30.0)?;
        let initial_nutrients = r.range(0.0, 5.0)?;
        let nutrient_inputs = r.range(0.0, 1e100)?;
        let next_source_id = r.u32()?;
        ensure(next_source_id > 0, "invalid source sequence")?;
        let count = r.count(MAX_SOURCES)?;
        let mut sources = Vec::with_capacity(count);
        let mut ids = std::collections::HashSet::new();
        for _ in 0..count {
            let id = r.u32()?;
            let kind = r.u32()?;
            ensure(
                id > 0 && id < next_source_id && ids.insert(id) && kind <= 3,
                "invalid environment source",
            )?;
            let x = r.range(0.0, WIDTH)?;
            let y = r.range(0.0, HEIGHT)?;
            ensure(x < WIDTH && y < HEIGHT, "source is outside the habitat")?;
            let strength = r.range(if kind == 0 || kind == 3 { 0.0 } else { -1.0 }, 1.0)?;
            let radius = r.range(40.0, 400.0)?;
            sources.push(Source {
                id,
                kind,
                x,
                y,
                strength,
                radius,
            });
        }
        fn field(r: &mut Reader<'_>, low: f64, high: f64) -> Result<Vec<f64>> {
            ensure(
                r.count(PATCHES)? == PATCHES,
                "incorrect environment grid size",
            )?;
            (0..PATCHES).map(|_| r.range(low, high)).collect()
        }
        Ok(Self {
            ambient_temperature,
            ambient_light,
            current_strength,
            initial_nutrients,
            sources,
            temperature: field(r, 0.0, 1.0)?,
            light: field(r, 0.0, 1.0)?,
            heating: field(r, -2.0, 2.0)?,
            nutrients: field(r, 0.0, 1e100)?,
            nutrient_injection: field(r, 0.0, 24.0)?,
            organic: field(r, 0.0, 1e6)?,
            nutrient_inputs,
            next_source_id,
        })
    }
}
