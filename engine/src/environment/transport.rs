//! Shared scalar interpolation geometry. Applying a cached stencil preserves
//! the separate no-wall and wall-aware arithmetic of the original sampler.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum TransportStencil {
    Origin(usize),
    Bilinear {
        indices: [usize; 4],
        fx: f64,
        fy: f64,
    },
    Visible {
        indices: [usize; 4],
        weights: [f64; 4],
        total: f64,
    },
}
impl Default for TransportStencil {
    fn default() -> Self {
        Self::Origin(0)
    }
}
impl TransportStencil {
    pub(super) fn sample(&self, values: &[f64]) -> f64 {
        match self {
            Self::Origin(origin) => values[*origin],
            Self::Bilinear { indices, fx, fy } => {
                let a = values[indices[0]] * (1.0 - fx) + values[indices[1]] * fx;
                let b = values[indices[2]] * (1.0 - fx) + values[indices[3]] * fx;
                a * (1.0 - fy) + b * fy
            }
            Self::Visible {
                indices,
                weights,
                total,
            } => {
                let mut value = 0.0;
                for index in 0..4 {
                    if weights[index] > 0.0 {
                        value += values[indices[index]] * weights[index];
                    }
                }
                value / total
            }
        }
    }
}

impl Environment {
    pub(super) fn prepare_scalar_transport(&mut self, time: f64) {
        if self.current_strength == 0.0 && self.scratch.transport.len() == self.patch_count() {
            return;
        }
        self.scratch
            .transport
            .resize(self.patch_count(), TransportStencil::default());
        for i in 0..self.patch_count() {
            let (x, y) = self.patch_position(i);
            let (vx, vy) = self.grid_flow(i, 0, time);
            self.scratch.transport[i] = self.transport_stencil(i, x - vx * DT, y - vy * DT);
        }
    }
    pub(super) fn transport_stencil(&self, origin: usize, x: f64, y: f64) -> TransportStencil {
        let walls = !self.barriers.is_empty();
        let from = self.patch_position(origin);
        let local = (x - from.0).abs() <= PATCH_SIZE && (y - from.1).abs() <= PATCH_SIZE;
        let clear = walls && local && self.clear_wall_neighborhood(origin);
        if walls && (delta(x - from.0, self.width).abs() + delta(y - from.1, self.height).abs() < 1e-12
            || (!clear && self.transport_segment_blocked(origin, (x, y)))) {
            return TransportStencil::Origin(origin);
        }
        let gx = wrapped(x, self.width) / PATCH_SIZE - 0.5;
        let gy = wrapped(y, self.height) / PATCH_SIZE - 0.5;
        let ix = gx.floor() as isize;
        let iy = gy.floor() as isize;
        let fx = gx - gx.floor();
        let fy = gy - gy.floor();
        // Only the seam needs integer wrapping. Neighbors are shared by the
        // four corners; the common case has no division or remainder here.
        let col = if ix < 0 { self.columns - 1 } else { ix as usize };
        let row = if iy < 0 { self.rows - 1 } else { iy as usize };
        let top_left = row * self.columns + col;
        let [_, top_right, _, bottom_left] = self.grid.neighbors[top_left];
        let bottom_right = self.grid.neighbors[bottom_left][1];
        let indices = [top_left, top_right, bottom_left, bottom_right];
        if !walls {
            return TransportStencil::Bilinear { indices, fx, fy };
        }
        let mut weights = [
            (1.0 - fx) * (1.0 - fy),
            fx * (1.0 - fy),
            (1.0 - fx) * fy,
            fx * fy,
        ];
        let mut total = 0.0;
        for corner in 0..4 {
            if weights[corner] <= 0.0 {
                continue;
            }
            if clear || indices[corner] == origin
                || !self.transport_neighbor_blocked(origin, indices[corner])
            {
                total += weights[corner];
            } else {
                weights[corner] = 0.0;
            }
        }
        if total > 0.0 {
            TransportStencil::Visible {
                indices,
                weights,
                total,
            }
        } else {
            TransportStencil::Origin(origin)
        }
    }
}
