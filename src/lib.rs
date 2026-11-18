#![no_std]
#![forbid(unsafe_code)]
//! # embassy-ssd1306-3d
//!
//! A `no_std` wireframe 3D engine for SSD1306 OLED displays (128×64).
//!
//! Uses [`embedded_vector::Vec3`] for vertices,
//! [`embedded_matrix::Matrix3x3`] for rotations,
//! and [`embedded_trig_f32`] for sine and cosine.
//!
//! ## Pipeline
//!
//! ```text
//! Vec3 → Matrix3x3 (rotation) → Camera::project() → (i32, i32) → line()
//! ```
//!
//! ## Minimal example
//!
//! ```rust,no_run
//! use embedded_vector::Vec3;
//! use embassy_ssd1306_3d::{cube, Camera, rotation_xyz};
//! use embassy_ssd1306_graphics::Graphics;
//!
//! let cam  = Camera::new(64.0, 32.0, 80.0);
//! let mesh = cube(18.0);
//!
//! oled.clear();
//! {
//!     let mut gfx = Graphics::new(&mut oled);
//!     let rot = rotation_xyz(ax, ay, az);
//!     mesh.draw_wireframe(&mut gfx, &cam, &rot);
//! }
//! oled.flush().await.unwrap();
//! ```

//use embassy_ssd1306::Ssd1306;
use embassy_ssd1306_graphics::{line, Graphics};
use embedded_hal_async::i2c::I2c;
use embedded_matrix::Matrix3x3;
use embedded_trig_f32::{cos, sin};
use embedded_vector::Vec3;

// ─────────────────────────────────────────────────────────────────────────────
// Vec3 × Matrix3x3 transformation
// ─────────────────────────────────────────────────────────────────────────────

/// Applies a `Matrix3x3` rotation to a `Vec3`.
///
/// `embedded-vector` does not depend on `embedded-matrix`, so this free
/// function is provided instead of implementing a foreign trait.
#[inline]
pub fn transform(v: &Vec3, m: &Matrix3x3) -> Vec3 {
    Vec3::new(
        m.get(0, 0) * v.x + m.get(0, 1) * v.y + m.get(0, 2) * v.z,
        m.get(1, 0) * v.x + m.get(1, 1) * v.y + m.get(1, 2) * v.z,
        m.get(2, 0) * v.x + m.get(2, 1) * v.y + m.get(2, 2) * v.z,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Rotation matrices
// ─────────────────────────────────────────────────────────────────────────────

/// Rotation around the X axis.
pub fn rotation_x(a: f32) -> Matrix3x3 {
    Matrix3x3::new([
        [1.0,    0.0,     0.0],
        [0.0,  cos(a), -sin(a)],
        [0.0,  sin(a),  cos(a)],
    ])
}

/// Rotation around the Y axis.
pub fn rotation_y(a: f32) -> Matrix3x3 {
    Matrix3x3::new([
        [ cos(a), 0.0, sin(a)],
        [   0.0,  1.0,   0.0 ],
        [-sin(a), 0.0, cos(a)],
    ])
}

/// Rotation around the Z axis.
pub fn rotation_z(a: f32) -> Matrix3x3 {
    Matrix3x3::new([
        [cos(a), -sin(a), 0.0],
        [sin(a),  cos(a), 0.0],
        [  0.0,     0.0,  1.0],
    ])
}

/// Rotation combinée Rx · Ry · Rz.
#[inline]
pub fn rotation_xyz(ax: f32, ay: f32, az: f32) -> Matrix3x3 {
    rotation_x(ax) * rotation_y(ay) * rotation_z(az)
}

// ─────────────────────────────────────────────────────────────────────────────
// Perspective camera projection
// ─────────────────────────────────────────────────────────────────────────────

/// Perspective camera centered on a 128×64 screen.
///
/// Projection: `sx = cx + focal * X / (Z + focal)`.
/// Returns `None` if the point is behind the camera plane (`Z + focal ≤ 0`).
pub struct Camera {
    pub cx:    f32,
    pub cy:    f32,
    pub focal: f32,
}

impl Camera {
    #[inline]
    pub const fn new(cx: f32, cy: f32, focal: f32) -> Self {
        Self { cx, cy, focal }
    }

    /// Projects a [`Vec3`] to pixel coordinates `(x, y)`.
    #[inline]
    pub fn project(&self, v: &Vec3) -> Option<(i32, i32)> {
        let dz = v.z + self.focal;
        if dz <= 0.0 {
            return None;
        }
        Some((
            (self.cx + self.focal * v.x / dz) as i32,
            (self.cy + self.focal * v.y / dz) as i32,
        ))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge
// ─────────────────────────────────────────────────────────────────────────────

/// An edge connecting two vertices by their index in `Mesh::vertices`.
#[derive(Clone, Copy)]
pub struct Edge(pub usize, pub usize);

// ─────────────────────────────────────────────────────────────────────────────
// Mesh
// ─────────────────────────────────────────────────────────────────────────────

/// A wireframe 3D mesh with fixed-size arrays and zero allocations.
///
/// `V` = number of vertices, `E` = number of edges.
pub struct Mesh<const V: usize, const E: usize> {
    pub vertices: [Vec3; V],
    pub edges:    [Edge; E],
}

impl<const V: usize, const E: usize> Mesh<V, E> {
    /// Draws the wireframe mesh.
    ///
    /// Projects all vertices using `transform` + `cam.project()`,
    /// then draws each visible edge using [`line()`].
    pub fn draw_wireframe<I: I2c>(
        &self,
        gfx: &mut Graphics<'_, I>,
        cam: &Camera,
        rot: &Matrix3x3,
    ) {
        let mut projected = [None::<(i32, i32)>; V];
        for (i, v) in self.vertices.iter().enumerate() {
            projected[i] = cam.project(&transform(v, rot));
        }
        for edge in &self.edges {
            if let (Some((x0, y0)), Some((x1, y1))) =
                (projected[edge.0], projected[edge.1])
            {
                line(gfx, x0, y0, x1, y1, true);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3D primitives
// ─────────────────────────────────────────────────────────────────────────────

/// Cube centered at the origin, with a half-side length of `s`. (8 vertices, 12 edges)
pub fn cube(s: f32) -> Mesh<8, 12> {
    Mesh {
        vertices: [
            Vec3::new(-s, -s, -s), // 0
            Vec3::new( s, -s, -s), // 1
            Vec3::new( s,  s, -s), // 2
            Vec3::new(-s,  s, -s), // 3
            Vec3::new(-s, -s,  s), // 4
            Vec3::new( s, -s,  s), // 5
            Vec3::new( s,  s,  s), // 6
            Vec3::new(-s,  s,  s), // 7
        ],
        edges: [
            Edge(0,1), Edge(1,2), Edge(2,3), Edge(3,0), // back face
            Edge(4,5), Edge(5,6), Edge(6,7), Edge(7,4), // front face
            Edge(0,4), Edge(1,5), Edge(2,6), Edge(3,7), // side edges
        ],
    }
}

/// Square-based pyramid. (5 vertices, 8 edges)
pub fn pyramid(base: f32, height: f32) -> Mesh<5, 8> {
    let h2 = height / 2.0;
    Mesh {
        vertices: [
            Vec3::new(-base, -base, -h2), // 0
            Vec3::new( base, -base, -h2), // 1
            Vec3::new( base,  base, -h2), // 2
            Vec3::new(-base,  base, -h2), // 3
            Vec3::new(  0.0,   0.0,  h2), // 4 apex
        ],
        edges: [
            Edge(0,1), Edge(1,2), Edge(2,3), Edge(3,0), // base
            Edge(0,4), Edge(1,4), Edge(2,4), Edge(3,4), // rising edges
        ],
    }
}

/// Regular octahedron with radius `r`. (6 vertices, 12 edges)
pub fn octahedron(r: f32) -> Mesh<6, 12> {
    Mesh {
        vertices: [
            Vec3::new( r,   0.0,  0.0), // 0
            Vec3::new(-r,   0.0,  0.0), // 1
            Vec3::new( 0.0,  r,   0.0), // 2
            Vec3::new( 0.0, -r,   0.0), // 3
            Vec3::new( 0.0,  0.0,  r),  // 4
            Vec3::new( 0.0,  0.0, -r),  // 5
        ],
        edges: [
            Edge(0,2), Edge(2,1), Edge(1,3), Edge(3,0),
            Edge(0,4), Edge(2,4), Edge(1,4), Edge(3,4),
            Edge(0,5), Edge(2,5), Edge(1,5), Edge(3,5),
        ],
    }
}

/// Approximation of a regular icosahedron. (12 vertices, 30 edges)
///
/// Uses the golden ratio φ ≈ 1.618 to position the vertices.
pub fn icosahedron(r: f32) -> Mesh<12, 30> {
    let phi = 1.618_034 * r;
    let one = r;
    Mesh {
        vertices: [
            Vec3::new( 0.0,  one,  phi), // 0
            Vec3::new( 0.0, -one,  phi), // 1
            Vec3::new( 0.0,  one, -phi), // 2
            Vec3::new( 0.0, -one, -phi), // 3
            Vec3::new( one,  phi,  0.0), // 4
            Vec3::new(-one,  phi,  0.0), // 5
            Vec3::new( one, -phi,  0.0), // 6
            Vec3::new(-one, -phi,  0.0), // 7
            Vec3::new( phi,  0.0,  one), // 8
            Vec3::new(-phi,  0.0,  one), // 9
            Vec3::new( phi,  0.0, -one), // 10
            Vec3::new(-phi,  0.0, -one), // 11
        ],
        edges: [
            Edge(0,1),  Edge(0,4),  Edge(0,5),  Edge(0,8),  Edge(0,9),
            Edge(1,6),  Edge(1,7),  Edge(1,8),  Edge(1,9),
            Edge(2,3),  Edge(2,4),  Edge(2,5),  Edge(2,10), Edge(2,11),
            Edge(3,6),  Edge(3,7),  Edge(3,10), Edge(3,11),
            Edge(4,5),  Edge(4,8),  Edge(4,10),
            Edge(5,9),  Edge(5,11),
            Edge(6,7),  Edge(6,8),  Edge(6,10),
            Edge(7,9),  Edge(7,11),
            Edge(8,10), Edge(9,11),
        ],
    }
}