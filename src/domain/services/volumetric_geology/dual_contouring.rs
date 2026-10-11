//! Dual Contouring isosurface extraction over a local voxel lattice.
//!
//! Given a node-centred scalar density grid and its analytic gradients, this
//! extracts the zero isosurface as triangles. Each cell that straddles the
//! surface places one vertex by minimizing the Quadratic Error Function (QEF)
//! of its edge crossings, which preserves sharp cliff edges and angular
//! fracture planes that marching cubes would round off. Adjacent cell vertices
//! are joined by a quad around every sign-changing lattice edge.
//!
//! Reference: Ju et al., "Dual Contouring of Hermite Data" (2002). The
//! implementation is pure domain code and deterministic: iteration order is
//! fixed and no floating-point reduction order depends on the platform.

use crate::domain::math::DVec3;

use super::DensityField;

/// A node-centred density/gradient lattice in patch-local metres (`x` east,
/// `y` north, `z` altitude above the mean radius).
pub struct VoxelGrid {
    dims: [usize; 3],
    origin: DVec3,
    cell_m: f64,
    densities: Vec<f64>,
    gradients: Vec<DVec3>,
}

impl VoxelGrid {
    /// Number of lattice nodes for a `dims` grid.
    pub fn node_count(dims: [usize; 3]) -> usize {
        dims[0] * dims[1] * dims[2]
    }

    /// Assemble a grid from precomputed node densities and unit gradients.
    /// `densities` and `gradients` are x-fastest, then y, then z.
    pub fn new(
        dims: [usize; 3],
        origin: DVec3,
        cell_m: f64,
        densities: Vec<f64>,
        gradients: Vec<DVec3>,
    ) -> Self {
        assert!(
            dims.iter().all(|d| *d >= 2),
            "voxel grid needs at least two nodes per axis"
        );
        let expected = Self::node_count(dims);
        assert_eq!(densities.len(), expected, "density count must match dims");
        assert_eq!(gradients.len(), expected, "gradient count must match dims");
        assert!(cell_m > 0.0, "voxel cell size must be positive");
        Self {
            dims,
            origin,
            cell_m,
            densities,
            gradients,
        }
    }

    /// Sample a density field onto a node lattice. Gradients use central
    /// differences at half a cell, matching the extraction resolution.
    pub fn sample(field: &DensityField<'_>, origin: DVec3, cell_m: f64, dims: [usize; 3]) -> Self {
        let mut densities = Vec::with_capacity(Self::node_count(dims));
        let mut gradients = Vec::with_capacity(Self::node_count(dims));
        let step = cell_m * 0.5;
        for k in 0..dims[2] {
            for j in 0..dims[1] {
                for i in 0..dims[0] {
                    let p = origin + DVec3::new(i as f64, j as f64, k as f64) * cell_m;
                    densities.push(field.density(p));
                    gradients.push(field.gradient(p, step));
                }
            }
        }
        Self::new(dims, origin, cell_m, densities, gradients)
    }

    pub fn dims(&self) -> [usize; 3] {
        self.dims
    }

    pub fn cell_m(&self) -> f64 {
        self.cell_m
    }

    fn index(&self, i: usize, j: usize, k: usize) -> usize {
        (k * self.dims[1] + j) * self.dims[0] + i
    }

    fn position(&self, i: usize, j: usize, k: usize) -> DVec3 {
        self.origin + DVec3::new(i as f64, j as f64, k as f64) * self.cell_m
    }

    fn density(&self, i: usize, j: usize, k: usize) -> f64 {
        self.densities[self.index(i, j, k)]
    }

    fn gradient(&self, i: usize, j: usize, k: usize) -> DVec3 {
        self.gradients[self.index(i, j, k)]
    }
}

/// The triangle mesh extracted from a [`VoxelGrid`], in patch-local metres.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedSurface {
    pub vertices: Vec<DVec3>,
    pub indices: Vec<u32>,
}

impl ExtractedSurface {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Cell corners, ordered so that bit 0 is +x, bit 1 is +y, bit 2 is +z.
const CORNERS: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];

/// The twelve cell edges as corner-index pairs.
const EDGES: [[usize; 2]; 12] = [
    [0, 1],
    [2, 3],
    [4, 5],
    [6, 7],
    [0, 2],
    [1, 3],
    [4, 6],
    [5, 7],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];

/// Extract the zero isosurface from a density grid with Dual Contouring.
pub fn extract_surface(grid: &VoxelGrid) -> ExtractedSurface {
    let [nx, ny, nz] = grid.dims();
    let cells = [nx - 1, ny - 1, nz - 1];
    let cell_count = cells[0] * cells[1] * cells[2];

    let cell_index = |i: usize, j: usize, k: usize| (k * cells[1] + j) * cells[0] + i;

    let mut cell_vertex: Vec<Option<u32>> = vec![None; cell_count];
    let mut vertices: Vec<DVec3> = Vec::new();
    for k in 0..cells[2] {
        for j in 0..cells[1] {
            for i in 0..cells[0] {
                if let Some(vertex) = cell_vertex_position(grid, i, j, k) {
                    cell_vertex[cell_index(i, j, k)] = Some(vertices.len() as u32);
                    vertices.push(vertex);
                }
            }
        }
    }

    let mut indices: Vec<u32> = Vec::new();
    // Every interior sign-changing lattice edge joins the four cells around it.
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                // +x edge: perpendicular plane is (y, z).
                if i + 1 < nx && j >= 1 && k >= 1 {
                    emit_quad(
                        grid,
                        &cell_vertex,
                        cells,
                        &cell_index,
                        [i, j, k],
                        [i + 1, j, k],
                        DVec3::X,
                        [[i, j - 1, k - 1], [i, j, k - 1], [i, j, k], [i, j - 1, k]],
                        &vertices,
                        &mut indices,
                    );
                }
                // +y edge: perpendicular plane is (x, z).
                if j + 1 < ny && i >= 1 && k >= 1 {
                    emit_quad(
                        grid,
                        &cell_vertex,
                        cells,
                        &cell_index,
                        [i, j, k],
                        [i, j + 1, k],
                        DVec3::Y,
                        [[i - 1, j, k - 1], [i, j, k - 1], [i, j, k], [i - 1, j, k]],
                        &vertices,
                        &mut indices,
                    );
                }
                // +z edge: perpendicular plane is (x, y).
                if k + 1 < nz && i >= 1 && j >= 1 {
                    emit_quad(
                        grid,
                        &cell_vertex,
                        cells,
                        &cell_index,
                        [i, j, k],
                        [i, j, k + 1],
                        DVec3::Z,
                        [[i - 1, j - 1, k], [i, j - 1, k], [i, j, k], [i - 1, j, k]],
                        &vertices,
                        &mut indices,
                    );
                }
            }
        }
    }

    ExtractedSurface { vertices, indices }
}

#[allow(clippy::too_many_arguments)]
fn emit_quad(
    grid: &VoxelGrid,
    cell_vertex: &[Option<u32>],
    cells: [usize; 3],
    cell_index: &impl Fn(usize, usize, usize) -> usize,
    start: [usize; 3],
    end: [usize; 3],
    axis: DVec3,
    corners: [[usize; 3]; 4],
    vertices: &[DVec3],
    indices: &mut Vec<u32>,
) {
    let d_start = grid.density(start[0], start[1], start[2]);
    let d_end = grid.density(end[0], end[1], end[2]);
    if (d_start < 0.0) == (d_end < 0.0) {
        return;
    }
    let mut quad = [0u32; 4];
    for (slot, corner) in corners.iter().enumerate() {
        let [ci, cj, ck] = *corner;
        if ci >= cells[0] || cj >= cells[1] || ck >= cells[2] {
            return;
        }
        match cell_vertex[cell_index(ci, cj, ck)] {
            Some(vertex) => quad[slot] = vertex,
            None => return,
        }
    }

    // Outward normal is opposite the density gradient; along the edge that is
    // `-sign(d_end - d_start)` on the axis. Flip the winding if the quad normal
    // disagrees so faces always point from rock into air.
    let outward = axis * -(d_end - d_start).signum();
    let normal = (vertices[quad[1] as usize] - vertices[quad[0] as usize])
        .cross(vertices[quad[2] as usize] - vertices[quad[0] as usize]);
    if normal.dot(outward) >= 0.0 {
        indices.extend_from_slice(&[quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]]);
    } else {
        indices.extend_from_slice(&[quad[0], quad[3], quad[2], quad[0], quad[2], quad[1]]);
    }
}

/// Solve the cell QEF and clamp the result to the cell bounding box.
fn cell_vertex_position(grid: &VoxelGrid, ci: usize, cj: usize, ck: usize) -> Option<DVec3> {
    let mut qef = Qef::new();
    for [a, b] in EDGES {
        let ca = CORNERS[a];
        let cb = CORNERS[b];
        let (ia, ja, ka) = (ci + ca[0], cj + ca[1], ck + ca[2]);
        let (ib, jb, kb) = (ci + cb[0], cj + cb[1], ck + cb[2]);
        let da = grid.density(ia, ja, ka);
        let db = grid.density(ib, jb, kb);
        if (da < 0.0) == (db < 0.0) {
            continue;
        }
        let denom = da - db;
        let t = if denom.abs() > f64::MIN_POSITIVE {
            (da / denom).clamp(0.0, 1.0)
        } else {
            0.5
        };
        let pa = grid.position(ia, ja, ka);
        let pb = grid.position(ib, jb, kb);
        let point = pa + (pb - pa) * t;
        let na = grid.gradient(ia, ja, ka);
        let nb = grid.gradient(ib, jb, kb);
        let normal = (na + (nb - na) * t).normalize_or_zero();
        let normal = if normal.length_squared() > 0.0 {
            normal
        } else if na.length_squared() > 0.0 {
            na
        } else {
            nb
        };
        qef.add(point, normal);
    }

    let min = grid.position(ci, cj, ck);
    let max = min + DVec3::splat(grid.cell_m());
    qef.solve(min, max)
}

/// A quadratic error function accumulated from Hermite edge samples.
#[derive(Debug, Clone, Copy)]
struct Qef {
    ata: [[f64; 3]; 3],
    atb: [f64; 3],
    mass: DVec3,
    count: u32,
}

impl Qef {
    fn new() -> Self {
        Self {
            ata: [[0.0; 3]; 3],
            atb: [0.0; 3],
            mass: DVec3::ZERO,
            count: 0,
        }
    }

    fn add(&mut self, point: DVec3, normal: DVec3) {
        let n = [normal.x, normal.y, normal.z];
        for i in 0..3 {
            for j in 0..3 {
                self.ata[i][j] += n[i] * n[j];
            }
        }
        let d = normal.dot(point);
        for (b, n_i) in self.atb.iter_mut().zip(n) {
            *b += d * n_i;
        }
        self.mass += point;
        self.count += 1;
    }

    fn mass_point(&self) -> DVec3 {
        self.mass / self.count as f64
    }

    /// Minimize `E(x)` subject to a small regularization toward the mass point,
    /// then clamp into `[min, max]`. Falls back to the mass point when the
    /// normal matrix is rank-deficient (for example a single plane, where the
    /// solution is a whole line).
    fn solve(&self, min: DVec3, max: DVec3) -> Option<DVec3> {
        if self.count == 0 {
            return None;
        }
        const REGULARIZATION: f64 = 1.0e-3;
        let mass = self.mass_point();
        let mut a = self.ata;
        let mut b = self.atb;
        for i in 0..3 {
            a[i][i] += REGULARIZATION;
            b[i] += REGULARIZATION * mass[i];
        }
        let solution = solve3(a, b).map_or(mass, DVec3::from_array);
        Some(DVec3::new(
            solution.x.clamp(min.x, max.x),
            solution.y.clamp(min.y, max.y),
            solution.z.clamp(min.z, max.z),
        ))
    }
}

/// Solve a 3×3 linear system with partial-pivoting Gaussian elimination.
// The inner loop indexes two rows of the same matrix, so an iterator form would
// require split_at_mut for no clarity gain.
#[allow(clippy::needless_range_loop)]
fn solve3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let mut pivot = col;
        for row in col + 1..3 {
            if a[row][col].abs() > a[pivot][col].abs() {
                pivot = row;
            }
        }
        if a[pivot][col].abs() < 1.0e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..3 {
            let factor = a[row][col] / a[col][col];
            for c in col..3 {
                a[row][c] -= factor * a[col][c];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = [0.0f64; 3];
    for i in (0..3).rev() {
        let mut sum = b[i];
        for j in i + 1..3 {
            sum -= a[i][j] * x[j];
        }
        x[i] = sum / a[i][i];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::{ElevationBounds, TerrainSource};
    use std::fmt::Debug;

    /// Analytic density `z0 - z`: solid half-space below a horizontal plane,
    /// with the exact unit gradient `(0, 0, -1)`.
    fn plane_density(p: DVec3, z0: f64) -> (f64, DVec3) {
        (z0 - p.z, DVec3::new(0.0, 0.0, -1.0))
    }

    /// Analytic density `min(x0 - x, z0 - z)`: solid convex corner with a sharp
    /// 90° edge along +y, gradient chosen from the binding half-space.
    fn corner_density(p: DVec3, x0: f64, z0: f64) -> (f64, DVec3) {
        let a = x0 - p.x;
        let b = z0 - p.z;
        if a < b {
            (a, DVec3::new(-1.0, 0.0, 0.0))
        } else {
            (b, DVec3::new(0.0, 0.0, -1.0))
        }
    }

    fn grid_from(
        dims: [usize; 3],
        origin: DVec3,
        cell: f64,
        f: impl Fn(DVec3) -> (f64, DVec3),
    ) -> VoxelGrid {
        let n = VoxelGrid::node_count(dims);
        let mut densities = Vec::with_capacity(n);
        let mut gradients = Vec::with_capacity(n);
        for k in 0..dims[2] {
            for j in 0..dims[1] {
                for i in 0..dims[0] {
                    let p = origin + DVec3::new(i as f64, j as f64, k as f64) * cell;
                    let (d, g) = f(p);
                    densities.push(d);
                    gradients.push(g);
                }
            }
        }
        VoxelGrid::new(dims, origin, cell, densities, gradients)
    }

    #[test]
    fn qef_recovers_a_plane() {
        let mut qef = Qef::new();
        for (x, y) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            qef.add(DVec3::new(x, y, 1.0), DVec3::Z);
        }
        let v = qef.solve(DVec3::ZERO, DVec3::splat(2.0)).unwrap();
        assert!((v.z - 1.0).abs() < 1e-6);
        assert!((v.x - 0.5).abs() < 1e-6);
        assert!((v.y - 0.5).abs() < 1e-6);
    }

    #[test]
    fn extract_plane_faces_outward() {
        let grid = grid_from([6, 6, 6], DVec3::new(-1.0, -1.0, 0.0), 0.5, |p| {
            plane_density(p, 1.2)
        });
        let surface = extract_surface(&grid);
        assert!(!surface.is_empty());
        for v in &surface.vertices {
            assert!(
                (v.z - 1.2).abs() <= grid.cell_m() + 1e-9,
                "vertex off plane: {v:?}"
            );
        }
        // Every triangle normal points up, from rock (below) into air (above).
        for tri in surface.indices.as_chunks::<3>().0 {
            let a = surface.vertices[tri[0] as usize];
            let b = surface.vertices[tri[1] as usize];
            let c = surface.vertices[tri[2] as usize];
            let normal = (b - a).cross(c - a);
            assert!(normal.z > 0.0, "triangle winding is inverted");
        }
    }

    #[test]
    fn dc_preserves_sharp_corner() {
        let (x0, z0, cell) = (1.25, 1.25, 0.5);
        let grid = grid_from([8, 4, 8], DVec3::new(0.0, -1.0, 0.0), cell, |p| {
            corner_density(p, x0, z0)
        });
        let surface = extract_surface(&grid);
        assert!(!surface.is_empty());
        let corner = DVec3::new(x0, 0.0, z0);
        let nearest = surface
            .vertices
            .iter()
            .map(|v| DVec3::new(v.x, 0.0, v.z).distance(corner))
            .fold(f64::INFINITY, f64::min);
        assert!(
            nearest < cell * 0.5,
            "Dual Contouring vertex {nearest} did not snap to the sharp corner"
        );
    }

    #[test]
    fn extraction_is_deterministic() {
        let grid = grid_from([7, 7, 7], DVec3::ZERO, 0.4, |p| plane_density(p, 1.3));
        assert_eq!(extract_surface(&grid), extract_surface(&grid));
    }

    #[test]
    fn sampled_field_extracts_a_finite_surface() {
        #[derive(Debug)]
        struct Flat(f64);
        impl TerrainSource for Flat {
            fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
                self.0
            }
            fn elevation_bounds_m(&self) -> ElevationBounds {
                ElevationBounds::new(self.0, self.0)
            }
        }
        let source = Flat(0.0);
        let frame = super::super::PatchFrame::new(DVec3::new(0.1, 0.2, 0.97), 6_371_000.0);
        let field = DensityField::new(
            &source,
            frame,
            super::super::VolumetricGeologyConfig::default(),
        );
        let grid = VoxelGrid::sample(&field, DVec3::new(-2.0, -2.0, -2.0), 0.5, [9, 9, 9]);
        let surface = extract_surface(&grid);
        assert!(!surface.is_empty());
        assert!(surface.vertices.iter().all(|v| v.is_finite()));
        assert!(surface
            .indices
            .iter()
            .all(|i| (*i as usize) < surface.vertices.len()));
    }
}
