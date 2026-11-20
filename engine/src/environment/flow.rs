//! Derived flow tables. Source contributions retain their original addition
//! order; only the final vector is capped. None of these tables are serialized.
use super::*;

#[derive(Clone, Default)]
pub(super) struct FlowCache {
    sources: Vec<FlowSource>,
    combined: Vec<[(f64, f64); 3]>,
    phase: GridPhase,
}
impl FlowCache {
    pub(super) fn new(patches: usize) -> Self {
        Self {
            combined: vec![[(0.0, 0.0); 3]; patches],
            ..Self::default()
        }
    }
}

#[derive(Clone)]
struct FlowSource {
    source: Source,
    cosine: f64,
    sine: f64,
    radius_squared: f64,
    scale: f64,
    samples: Vec<[(f64, f64); 3]>,
}
impl FlowSource {
    fn new(source: &Source) -> Self {
        Self {
            source: source.clone(),
            cosine: source.angle.cos(),
            sine: source.angle.sin(),
            radius_squared: source.radius.powi(2),
            scale: if matches!(source.kind, 2 | 7) {
                source.strength * 60.0 / source.radius
            } else {
                source.strength * 60.0
            },
            samples: Vec::new(),
        }
    }
    fn contribution(&self, x: f64, y: f64, width: f64, height: f64) -> (f64, f64) {
        let source = &self.source;
        let dx = delta(x - source.x, width);
        let dy = delta(y - source.y, height);
        match source.kind {
            2 | 7 => {
                let weight = self.scale * (-2.0 * (dx * dx + dy * dy) / self.radius_squared).exp();
                if source.kind == 2 {
                    (dy * weight, dx * weight)
                } else {
                    (dx * weight, dy * weight)
                }
            }
            5 => {
                let along = dx * self.cosine + dy * self.sine;
                let across = -dx * self.sine + dy * self.cosine;
                let weight = self.scale
                    * (-2.0 * (across / source.radius).powi(2)
                        - 2.0 * (along / (source.length * 0.5)).powi(2))
                    .exp();
                (self.cosine * weight, self.sine * weight)
            }
            6 => {
                let along = dx * self.cosine + dy * self.sine;
                let across = -dx * self.sine + dy * self.cosine;
                let end = (along.abs() - source.length * 0.5).max(0.0);
                let weight =
                    self.scale * (-2.0 * (across * across + end * end) / self.radius_squared).exp();
                (self.cosine * weight, self.sine * weight)
            }
            _ => unreachable!(),
        }
    }
    fn add(&self, velocity: &mut (f64, f64), contribution: (f64, f64)) {
        if self.source.kind == 2 {
            velocity.0 -= contribution.0;
        } else {
            velocity.0 += contribution.0;
        }
        velocity.1 += contribution.1;
    }
}

/// Centers and positive faces use two coordinates per row/column. Keep the
/// exact unwrapped endpoint (including x==width), not a periodic substitute.
#[derive(Clone, Default)]
pub(super) struct GridPhase {
    time: Option<u64>,
    column_sine: Vec<[f64; 2]>,
    column_cosine: Vec<[f64; 2]>,
    row_sine: Vec<[f64; 2]>,
    row_cosine: Vec<[f64; 2]>,
}
impl GridPhase {
    fn matches(&self, time: f64, columns: usize, rows: usize) -> bool {
        self.time == Some(time.to_bits())
            && self.column_sine.len() == columns
            && self.row_sine.len() == rows
    }
    fn prepare(&mut self, time: f64, width: f64, height: f64, columns: usize, rows: usize) {
        if self.matches(time, columns, rows) {
            return;
        }
        self.column_sine.resize(columns, [0.0; 2]);
        self.column_cosine.resize(columns, [0.0; 2]);
        self.row_sine.resize(rows, [0.0; 2]);
        self.row_cosine.resize(rows, [0.0; 2]);
        let phase = time * 0.04;
        for col in 0..columns {
            let x = (col as f64 + 0.5) * width / columns as f64;
            for (face, x) in [x, x + 10.0].into_iter().enumerate() {
                let a = TAU * x / width + phase;
                self.column_sine[col][face] = a.sin();
                self.column_cosine[col][face] = a.cos();
            }
        }
        for row in 0..rows {
            let y = (row as f64 + 0.5) * height / rows as f64;
            for (face, y) in [y, y + 10.0].into_iter().enumerate() {
                let b = TAU * y / height - phase;
                self.row_sine[row][face] = b.sin();
                self.row_cosine[row][face] = b.cos();
            }
        }
        self.time = Some(time.to_bits());
    }
}

fn capped(velocity: (f64, f64)) -> (f64, f64) {
    let (vx, vy) = velocity;
    // This interior box is far from the cap; returning the unchanged pair is
    // exactly the branch the original hypot comparison would select.
    if vx.abs() <= 30.0 && vy.abs() <= 30.0 {
        return velocity;
    }
    let speed = vx.hypot(vy);
    if speed > 60.0 {
        (vx * 60.0 / speed, vy * 60.0 / speed)
    } else {
        velocity
    }
}

impl Environment {
    pub(super) fn rebuild_flow_samples(&mut self) {
        self.scratch.face_flow.clear();
        let sources: Vec<_> = self
            .sources
            .iter()
            .filter(|source| matches!(source.kind, 2 | 5 | 6 | 7) && source.strength != 0.0)
            .map(FlowSource::new)
            .collect();
        self.flow.sources = sources;
        if self.current_strength == 0.0 {
            self.flow
                .combined
                .resize(self.patch_count(), [(0.0, 0.0); 3]);
            for i in 0..self.patch_count() {
                let (x, y) = self.patch_position(i);
                let mut samples = [(0.0, 0.0); 3];
                for (face, (x, y)) in [(x, y), (x + 10.0, y), (x, y + 10.0)]
                    .into_iter()
                    .enumerate()
                {
                    for source in &self.flow.sources {
                        source.add(
                            &mut samples[face],
                            source.contribution(x, y, self.width, self.height),
                        );
                    }
                    samples[face] = capped(samples[face]);
                }
                self.flow.combined[i] = samples;
            }
        } else {
            self.flow.combined.clear();
            let count = self.patch_count();
            let (width, height, columns, rows) = (self.width, self.height, self.columns, self.rows);
            for source in &mut self.flow.sources {
                source.samples.resize(count, [(0.0, 0.0); 3]);
                for i in 0..count {
                    let x = ((i % columns) as f64 + 0.5) * width / columns as f64;
                    let y = ((i / columns) as f64 + 0.5) * height / rows as f64;
                    source.samples[i] = [
                        source.contribution(x, y, width, height),
                        source.contribution(x + 10.0, y, width, height),
                        source.contribution(x, y + 10.0, width, height),
                    ];
                }
            }
        }
    }

    pub(super) fn prepare_grid_flow(&mut self, time: f64) {
        if self.current_strength != 0.0 {
            self.flow
                .phase
                .prepare(time, self.width, self.height, self.columns, self.rows);
        }
    }
    pub(super) fn snapshot_phase(&self, time: f64) -> GridPhase {
        let mut phase = GridPhase::default();
        if self.current_strength != 0.0 {
            phase.prepare(time, self.width, self.height, self.columns, self.rows);
        }
        phase
    }
    fn ambient_flow(&self, x: f64, y: f64, time: f64) -> (f64, f64) {
        if self.current_strength == 0.0 {
            return (0.0, 0.0);
        }
        let phase = time * 0.04;
        let a = TAU * x / self.width + phase;
        let b = TAU * y / self.height - phase;
        (
            self.current_strength * b.sin() * a.cos(),
            -self.current_strength * self.height / self.width * b.cos() * a.sin(),
        )
    }
    pub(super) fn raw_flow_at(&self, x: f64, y: f64, time: f64) -> (f64, f64) {
        let mut velocity = self.ambient_flow(x, y, time);
        for source in &self.flow.sources {
            source.add(
                &mut velocity,
                source.contribution(x, y, self.width, self.height),
            );
        }
        capped(velocity)
    }
    pub(super) fn grid_flow(&self, index: usize, face: usize, time: f64) -> (f64, f64) {
        self.grid_flow_with_phase(index, face, time, &self.flow.phase)
    }
    pub(super) fn grid_flow_with_phase(
        &self,
        index: usize,
        face: usize,
        time: f64,
        phase: &GridPhase,
    ) -> (f64, f64) {
        if face == 0 && self.solid_patches[index] {
            return (0.0, 0.0);
        }
        if self.current_strength == 0.0 {
            return self.flow.combined[index][face];
        }
        let mut velocity = if phase.matches(time, self.columns, self.rows) {
            let col = index % self.columns;
            let row = index / self.columns;
            let xf = usize::from(face == 1);
            let yf = usize::from(face == 2);
            (
                self.current_strength * phase.row_sine[row][yf] * phase.column_cosine[col][xf],
                -self.current_strength * self.height / self.width
                    * phase.row_cosine[row][yf]
                    * phase.column_sine[col][xf],
            )
        } else {
            let (mut x, mut y) = self.patch_position(index);
            if face == 1 {
                x += 10.0;
            } else if face == 2 {
                y += 10.0;
            }
            self.ambient_flow(x, y, time)
        };
        for source in &self.flow.sources {
            source.add(&mut velocity, source.samples[index][face]);
        }
        capped(velocity)
    }
}
