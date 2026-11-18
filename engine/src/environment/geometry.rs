//! Oriented solid obstacles and dimension-aware field geometry.
use super::*;

impl Environment {
    /// Resample by overlap of normalized patch areas. Mineral amounts are split,
    /// never synthesized or discarded. Scalar intensities remain area averages.
    pub fn set_size(&mut self, width: f64, height: f64) -> bool {
        if !width.is_finite()
            || !height.is_finite()
            || !(400.0..=4800.0).contains(&width)
            || !(400.0..=3040.0).contains(&height)
            || width % PATCH_SIZE != 0.0
            || height % PATCH_SIZE != 0.0
        {
            return false;
        }
        if width == self.width && height == self.height {
            return true;
        }
        if self
            .barriers
            .iter()
            .any(|b| !barrier_fits(b.length, b.width, b.angle, width, height))
        {
            return false;
        }
        let columns = (width / PATCH_SIZE) as usize;
        let rows = (height / PATCH_SIZE) as usize;
        let count = columns * rows;
        let mut minerals = vec![0.0; count];
        let mut heat = vec![0.0; count];
        let mut organic = vec![0.0; count];
        for old in 0..self.patch_count() {
            let ox = old % self.columns;
            let oy = old / self.columns;
            let left = ox as f64 * columns as f64 / self.columns as f64;
            let right = (ox + 1) as f64 * columns as f64 / self.columns as f64;
            let top = oy as f64 * rows as f64 / self.rows as f64;
            let bottom = (oy + 1) as f64 * rows as f64 / self.rows as f64;
            let area = (right - left) * (bottom - top);
            for y in top.floor() as usize..(bottom.ceil() as usize).min(rows) {
                for x in left.floor() as usize..(right.ceil() as usize).min(columns) {
                    let overlap = ((x + 1) as f64).min(right) - (x as f64).max(left);
                    let overlap = overlap * (((y + 1) as f64).min(bottom) - (y as f64).max(top));
                    if overlap <= 0.0 {
                        continue;
                    }
                    let index = y * columns + x;
                    minerals[index] += self.nutrients[old] * overlap / area;
                    heat[index] += self.temperature[old] * overlap;
                    organic[index] += self.organic[old] * overlap;
                }
            }
        }
        for value in &mut heat {
            *value = value.clamp(0.0, 1.0);
        }
        for value in &mut organic {
            *value = value.clamp(0.0, 1e6);
        }
        for source in &mut self.sources {
            source.x = (source.x * width / self.width).rem_euclid(width);
            source.y = (source.y * height / self.height).rem_euclid(height);
        }
        for barrier in &mut self.barriers {
            barrier.x = (barrier.x * width / self.width).rem_euclid(width);
            barrier.y = (barrier.y * height / self.height).rem_euclid(height);
        }
        self.width = width;
        self.height = height;
        self.columns = columns;
        self.rows = rows;
        self.nutrients = minerals;
        self.temperature = heat;
        self.organic = organic;
        self.light = vec![0.0; count];
        self.heating = vec![0.0; count];
        self.nutrient_injection = vec![0.0; count];
        self.nutrient_absorption = vec![0.0; count];
        self.blocked_right = vec![false; count];
        self.blocked_down = vec![false; count];
        self.solid_patches = vec![false; count];
        self.source_flow.clear();
        self.scratch = FieldScratch::default();
        self.rebuild_sources();
        self.rebuild_barriers();
        true
    }

    pub fn update_source(
        &mut self,
        id: u32,
        kind: u32,
        x: f64,
        y: f64,
        strength: f64,
        radius: f64,
        angle: f64,
        length: f64,
    ) -> bool {
        if kind > 7
            || ![x, y, strength, radius, angle, length]
                .iter()
                .all(|n| n.is_finite())
        {
            return false;
        }
        let Some(source) = self.sources.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        *source = Source {
            id,
            kind,
            x: x.rem_euclid(self.width),
            y: y.rem_euclid(self.height),
            strength: strength.clamp(if matches!(kind, 0 | 3 | 4) { 0.0 } else { -1.0 }, 1.0),
            radius: radius.clamp(20.0, 1000.0),
            angle: angle.rem_euclid(TAU),
            length: length.clamp(40.0, 4800.0),
        };
        self.rebuild_sources();
        true
    }

    pub fn add_barrier(
        &mut self,
        x: f64,
        y: f64,
        length: f64,
        width: f64,
        angle: f64,
        floating: bool,
    ) -> u32 {
        if self.barriers.len() >= MAX_BARRIERS
            || ![x, y, length, width, angle].iter().all(|n| n.is_finite())
        {
            return 0;
        }
        let length = length.clamp(20.0, 2400.0);
        let width = width.clamp(10.0, 400.0);
        if !barrier_fits(length, width, angle, self.width, self.height) {
            return 0;
        }
        let id = self.next_barrier_id;
        let Some(next) = id.checked_add(1) else {
            return 0;
        };
        self.next_barrier_id = next;
        self.barriers.push(Barrier {
            id,
            x: x.rem_euclid(self.width),
            y: y.rem_euclid(self.height),
            length: length.clamp(20.0, 2400.0),
            width: width.clamp(10.0, 400.0),
            angle: angle.rem_euclid(TAU),
            floating,
            vx: 0.0,
            vy: 0.0,
        });
        self.rebuild_barriers();
        id
    }
    pub fn update_barrier(
        &mut self,
        id: u32,
        x: f64,
        y: f64,
        length: f64,
        width: f64,
        angle: f64,
        floating: bool,
    ) -> bool {
        if ![x, y, length, width, angle].iter().all(|n| n.is_finite()) {
            return false;
        }
        let length = length.clamp(20.0, 2400.0);
        let width = width.clamp(10.0, 400.0);
        if !barrier_fits(length, width, angle, self.width, self.height) {
            return false;
        }
        let Some(b) = self.barriers.iter_mut().find(|b| b.id == id) else {
            return false;
        };
        *b = Barrier {
            id,
            x: x.rem_euclid(self.width),
            y: y.rem_euclid(self.height),
            length: length.clamp(20.0, 2400.0),
            width: width.clamp(10.0, 400.0),
            angle: angle.rem_euclid(TAU),
            floating,
            vx: 0.0,
            vy: 0.0,
        };
        self.rebuild_barriers();
        true
    }
    pub fn remove_barrier(&mut self, id: u32) -> bool {
        let old = self.barriers.len();
        self.barriers.retain(|b| b.id != id);
        if old == self.barriers.len() {
            return false;
        }
        self.rebuild_barriers();
        true
    }
    pub fn clear_barriers(&mut self) {
        self.barriers.clear();
        self.rebuild_barriers();
    }

    /// Segment/rectangle intersection in one unwrapped local image.
    fn barrier_hit(
        &self,
        barrier: &Barrier,
        from: (f64, f64),
        movement: (f64, f64),
        radius: f64,
    ) -> Option<(f64, f64, f64)> {
        let c = barrier.angle.cos();
        let s = barrier.angle.sin();
        let dx = delta(from.0 - barrier.x, self.width);
        let dy = delta(from.1 - barrier.y, self.height);
        let half = [barrier.length * 0.5 + radius, barrier.width * 0.5 + radius];
        let velocity = [
            movement.0 * c + movement.1 * s,
            -movement.0 * s + movement.1 * c,
        ];
        let mut closest: Option<(f64, f64, f64)> = None;
        for oy in [-self.height, 0.0, self.height] {
            for ox in [-self.width, 0.0, self.width] {
                let point = [
                    (dx + ox) * c + (dy + oy) * s,
                    -(dx + ox) * s + (dy + oy) * c,
                ];
                let mut entry: f64 = 0.0;
                let mut leave: f64 = 1.0;
                let mut normal = [0.0, 0.0];
                let mut intersects = true;
                for axis in 0..2 {
                    if velocity[axis].abs() < 1e-12 {
                        if point[axis].abs() > half[axis] {
                            intersects = false;
                            break;
                        }
                    } else {
                        let a = (-half[axis] - point[axis]) / velocity[axis];
                        let b = (half[axis] - point[axis]) / velocity[axis];
                        let near = a.min(b);
                        let far = a.max(b);
                        if near > entry {
                            entry = near;
                            normal = [0.0, 0.0];
                            normal[axis] = -velocity[axis].signum();
                        }
                        leave = leave.min(far);
                        if entry > leave {
                            intersects = false;
                            break;
                        }
                    }
                }
                if intersects && leave >= 0.0 && entry <= 1.0 {
                    // Starting inside is handled separately by projection. For line
                    // tests it still counts as occluded.
                    let nx = normal[0] * c - normal[1] * s;
                    let ny = normal[0] * s + normal[1] * c;
                    if closest.is_none_or(|v| entry < v.0) {
                        closest = Some((entry, nx, ny));
                    }
                }
            }
        }
        closest
    }
    pub fn segment_blocked(&self, from: (f64, f64), to: (f64, f64)) -> bool {
        let movement = (
            delta(to.0 - from.0, self.width),
            delta(to.1 - from.1, self.height),
        );
        self.barriers
            .iter()
            .any(|b| self.barrier_hit(b, from, movement, 0.0).is_some())
    }

    pub fn solid_at(&self, x: f64, y: f64) -> bool {
        self.barriers.iter().any(|b| {
            let dx = delta(x - b.x, self.width);
            let dy = delta(y - b.y, self.height);
            let c = b.angle.cos();
            let s = b.angle.sin();
            (dx * c + dy * s).abs() <= b.length * 0.5 && (-dx * s + dy * c).abs() <= b.width * 0.5
        })
    }

    /// Swept collision prevents tunneling through thin walls. Remaining motion
    /// slides along a wall. Placing a wall over existing matter pushes it clear.
    pub fn move_circle(&self, from: (f64, f64), to: (f64, f64), radius: f64) -> (f64, f64) {
        if self.barriers.is_empty() {
            return (to.0.rem_euclid(self.width), to.1.rem_euclid(self.height));
        }
        let mut position = from;
        for _ in 0..8 {
            let mut changed = false;
            for b in &self.barriers {
                let c = b.angle.cos();
                let s = b.angle.sin();
                let half = [
                    b.length * 0.5 + radius + 0.001,
                    b.width * 0.5 + radius + 0.001,
                ];
                for oy in [-self.height, 0.0, self.height] {
                    for ox in [-self.width, 0.0, self.width] {
                        let dx = delta(position.0 - b.x, self.width) + ox;
                        let dy = delta(position.1 - b.y, self.height) + oy;
                        let local = [dx * c + dy * s, -dx * s + dy * c];
                        if local[0].abs() < half[0] && local[1].abs() < half[1] {
                            let axis = if half[0] - local[0].abs() < half[1] - local[1].abs() {
                                0
                            } else {
                                1
                            };
                            let shift = half[axis] * if local[axis] < 0.0 { -1.0 } else { 1.0 }
                                - local[axis];
                            if axis == 0 {
                                position.0 += shift * c;
                                position.1 += shift * s;
                            } else {
                                position.0 -= shift * s;
                                position.1 += shift * c;
                            }
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut movement = (
            delta(to.0 - from.0, self.width),
            delta(to.1 - from.1, self.height),
        );
        for _ in 0..4 {
            let mut nearest: Option<(f64, f64, f64)> = None;
            for b in &self.barriers {
                if let Some(hit) = self.barrier_hit(b, position, movement, radius) {
                    if hit.1 == 0.0 && hit.2 == 0.0 {
                        continue;
                    }
                    if nearest.is_none_or(|old| hit.0 < old.0) {
                        nearest = Some(hit);
                    }
                }
            }
            let Some((t, nx, ny)) = nearest else {
                position.0 += movement.0;
                position.1 += movement.1;
                break;
            };
            let t = (t - 1e-7).max(0.0);
            position.0 += movement.0 * t + nx * 0.001;
            position.1 += movement.1 * t + ny * 0.001;
            movement.0 *= 1.0 - t;
            movement.1 *= 1.0 - t;
            let inward = (movement.0 * nx + movement.1 * ny).min(0.0);
            movement.0 -= inward * nx;
            movement.1 -= inward * ny;
            if movement.0.hypot(movement.1) < 1e-9 {
                break;
            }
        }
        (
            position.0.rem_euclid(self.width),
            position.1.rem_euclid(self.height),
        )
    }

    pub(super) fn rebuild_barriers(&mut self) {
        self.blocked_right.fill(false);
        self.blocked_down.fill(false);
        self.solid_patches.fill(false);
        // Rasterize only the bounding box of each rectangle, including both
        // neighbors at its edge. Periodic indexing handles seam-spanning walls.
        for b in &self.barriers {
            let extent_x =
                (b.angle.cos().abs() * b.length + b.angle.sin().abs() * b.width) * 0.5 + PATCH_SIZE;
            let extent_y =
                (b.angle.sin().abs() * b.length + b.angle.cos().abs() * b.width) * 0.5 + PATCH_SIZE;
            let first_x = ((b.x - extent_x) / PATCH_SIZE).floor() as isize;
            let first_y = ((b.y - extent_y) / PATCH_SIZE).floor() as isize;
            let cols = ((2.0 * extent_x / PATCH_SIZE).ceil() as usize + 2).min(self.columns);
            let rows = ((2.0 * extent_y / PATCH_SIZE).ceil() as usize + 2).min(self.rows);
            for row in 0..rows {
                for col in 0..cols {
                    let x = (first_x + col as isize).rem_euclid(self.columns as isize) as usize;
                    let y = (first_y + row as isize).rem_euclid(self.rows as isize) as usize;
                    let i = y * self.columns + x;
                    let point = self.patch_position(i);
                    let dx = delta(point.0 - b.x, self.width);
                    let dy = delta(point.1 - b.y, self.height);
                    let c = b.angle.cos();
                    let s = b.angle.sin();
                    if (dx * c + dy * s).abs() <= b.length * 0.5
                        && (-dx * s + dy * c).abs() <= b.width * 0.5
                    {
                        self.solid_patches[i] = true;
                    }
                    if self.barrier_hit(b, point, (PATCH_SIZE, 0.0), 0.0).is_some() {
                        self.blocked_right[i] = true;
                    }
                    if self.barrier_hit(b, point, (0.0, PATCH_SIZE), 0.0).is_some() {
                        self.blocked_down[i] = true;
                    }
                }
            }
        }
    }

    pub(super) fn step_barriers(&mut self, time: f64) {
        if !self.barriers.iter().any(|b| b.floating) {
            return;
        }
        let movements: Vec<_> = self
            .barriers
            .iter()
            .map(|b| {
                if b.floating {
                    let (fx, fy) = self.raw_flow_at(b.x, b.y, time);
                    let follow = 1.0 - (-0.7 * DT).exp();
                    (b.vx + (fx - b.vx) * follow, b.vy + (fy - b.vy) * follow)
                } else {
                    (0.0, 0.0)
                }
            })
            .collect();
        let mut changed = false;
        for (b, (vx, vy)) in self.barriers.iter_mut().zip(movements) {
            b.vx = vx;
            b.vy = vy;
            if vx.abs() + vy.abs() > 1e-12 {
                changed = true;
                b.x = (b.x + vx * DT).rem_euclid(self.width);
                b.y = (b.y + vy * DT).rem_euclid(self.height);
            }
        }
        // Resolve overlapping moving rectangles by the separating-axis test.
        // Static walls stay fixed; floating/floating pairs share displacement.
        for _ in 0..8 {
            let mut separated = false;
            for a in 0..self.barriers.len() {
                for b in a + 1..self.barriers.len() {
                    if !self.barriers[a].floating && !self.barriers[b].floating {
                        continue;
                    }
                    let left = &self.barriers[a];
                    let right = &self.barriers[b];
                    if let Some((penetration, normal)) =
                        barrier_overlap(left, right, self.width, self.height)
                    {
                        changed = true;
                        separated = true;
                        let count = u8::from(left.floating) + u8::from(right.floating);
                        let shift = (penetration + 0.001) / count as f64;
                        let (lefts, rights) = self.barriers.split_at_mut(b);
                        let left = &mut lefts[a];
                        let right = &mut rights[0];
                        if left.floating {
                            left.x = (left.x - normal.0 * shift).rem_euclid(self.width);
                            left.y = (left.y - normal.1 * shift).rem_euclid(self.height);
                            left.vx = 0.0;
                            left.vy = 0.0;
                        }
                        if right.floating {
                            right.x = (right.x + normal.0 * shift).rem_euclid(self.width);
                            right.y = (right.y + normal.1 * shift).rem_euclid(self.height);
                            right.vx = 0.0;
                            right.vy = 0.0;
                        }
                    }
                }
            }
            if !separated {
                break;
            }
        }
        if changed {
            self.rebuild_barriers();
        }
    }
}

pub(super) fn barrier_fits(
    length: f64,
    width: f64,
    angle: f64,
    world_width: f64,
    world_height: f64,
) -> bool {
    length * angle.cos().abs() + width * angle.sin().abs() <= world_width + 1e-3
        && length * angle.sin().abs() + width * angle.cos().abs() <= world_height + 1e-3
}

fn barrier_overlap(
    left: &Barrier,
    right: &Barrier,
    width: f64,
    height: f64,
) -> Option<(f64, (f64, f64))> {
    let dx = delta(right.x - left.x, width);
    let dy = delta(right.y - left.y, height);
    let axes = [
        (left.angle.cos(), left.angle.sin()),
        (-left.angle.sin(), left.angle.cos()),
        (right.angle.cos(), right.angle.sin()),
        (-right.angle.sin(), right.angle.cos()),
    ];
    let mut result: Option<(f64, (f64, f64))> = None;
    for oy in [-height, 0.0, height] {
        for ox in [-width, 0.0, width] {
            let mut smallest = f64::INFINITY;
            let mut normal = (0.0, 0.0);
            let mut overlap = true;
            for (nx, ny) in axes {
                let projection = |r: &Barrier| {
                    (nx * r.angle.cos() + ny * r.angle.sin()).abs() * r.length * 0.5
                        + (-nx * r.angle.sin() + ny * r.angle.cos()).abs() * r.width * 0.5
                };
                let distance = (dx + ox) * nx + (dy + oy) * ny;
                let penetration = projection(left) + projection(right) - distance.abs();
                if penetration <= 0.0 {
                    overlap = false;
                    break;
                }
                if penetration < smallest {
                    smallest = penetration;
                    let sign = if distance < 0.0 { -1.0 } else { 1.0 };
                    normal = (nx * sign, ny * sign);
                }
            }
            if overlap && result.is_none_or(|old| smallest < old.0) {
                result = Some((smallest, normal));
            }
        }
    }
    result
}
