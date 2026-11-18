# embassy-ssd1306-3d

A `no_std` wireframe 3D engine for SSD1306 OLED displays (128×64), built on top of [`embassy-ssd1306-graphics`](https://github.com/jorgeandrecastro/embassy-ssd1306-graphics).

Zero allocation · No unsafe code · Integer-friendly · Tested on RP2350 / ready for Cortex-M

---

## RISC-V support

This crate supports Embassy's RISC-V time backend through the `time-riscv` feature:

```toml
[dependencies]
embassy-ssd1306-3d = { version = "0.2", default-features = false, features = ["time-riscv"] }
embassy-ssd1306-graphics = { version = "0.2", default-features = false, features = ["time-riscv"] }
```

---

## Complete stack

```text
embassy-ssd1306-3d          ← this crate (3D → 2D)
    ├── embassy-ssd1306-graphics   (line, circle, triangle…)
    ├── embedded-vector            (Vec3)
    ├── embedded-matrix            (Matrix3x3 rotations)
    └── embedded-trig-f32          (sin, cos)
            ↓
    embassy-ssd1306  (framebuffer · I2C · flush)
            ↓
    OLED SSD1306 128×64
```

---

## Installation

```toml
[dependencies]
embassy-ssd1306-3d       = "0.2"
embassy-ssd1306-graphics = "0.2"
embedded-vector          = "0.1"
embassy-ssd1306          = "0.7"
embedded-trig-f32        = "0.3"
```

---

## Quick start

```rust
use embassy_ssd1306_3d::{cube, Camera, rotation_xyz};
use embassy_ssd1306_graphics::Graphics;

let cam  = Camera::new(64.0, 32.0, 80.0);  // cx, cy, focal length
let mesh = cube(18.0);                      // half-side length: 18 px

loop {
    oled.clear();
    {
        let mut gfx = Graphics::new(&mut oled);
        let rot = rotation_xyz(ax, ay, az);
        mesh.draw_wireframe(&mut gfx, &cam, &rot);
    }
    oled.flush().await.unwrap();
    angle += 0.05;
}
```

---

## API reference

### `Camera`

```rust
pub struct Camera { pub cx: f32, pub cy: f32, pub focal: f32 }
```

Projection center and focal length. The larger `focal` is, the less pronounced the perspective.

```rust
// Camera centered on a 128×64 screen, with a standard focal length
let cam = Camera::new(64.0, 32.0, 80.0);

// Project a point: None if it is behind the camera
let screen: Option<(i32, i32)> = cam.project(&v);
```

| `focal` | Visual effect |
|---------|-------------|
| 40      | Very wide angle, strong perspective |
| 80      | Standard, balanced |
| 150     | Nearly orthographic |

---

### Rotations

```rust
// Elementary rotations around each axis
let rx = rotation_x(angle);
let ry = rotation_y(angle);
let rz = rotation_z(angle);

// Combined rotation Rx · Ry · Rz (most common use)
let rot = rotation_xyz(ax, ay, az);
```

The matrices are [`Matrix3x3`](https://docs.rs/embedded-matrix) values and can be composed with `*`:

```rust
// Custom rotation: first Y, then Z
let rot = rotation_y(ay) * rotation_z(az);
```

---

### `transform`

```rust
pub fn transform(v: &Vec3, m: &Matrix3x3) -> Vec3
```

Applies a rotation to a `Vec3`. This is a free function because `embedded-vector` and `embedded-matrix` do not know about each other (the Rust orphan rule).

```rust
let v_world = transform(&Vec3::new(10.0, 0.0, 0.0), &rotation_y(angle));
```

---

### `Mesh<V, E>`

```rust
pub struct Mesh<const V: usize, const E: usize> {
    pub vertices: [Vec3; V],
    pub edges:    [Edge; E],
}
```

Sizes are known at compile time — no `Vec`, no heap allocation.

```rust
// Draw the complete wireframe in one line
mesh.draw_wireframe(&mut gfx, &cam, &rot);
```

**Built-in meshes:**

| Function | Vertices | Edges | Description |
|----------|---------|-------------|-------------|
| `cube(s)` | 8 | 12 | Cube, half-side length `s` |
| `pyramid(base, height)` | 5 | 8 | Square-based pyramid |
| `octahedron(r)` | 6 | 12 | Regular octahedron |
| `icosahedron(r)` | 12 | 30 | Icosahedron (golden ratio φ) |

```rust
let c = cube(18.0);
let p = pyramid(14.0, 28.0);
let o = octahedron(20.0);
let i = icosahedron(16.0);
```

---

### Custom mesh

```rust
use embedded_vector::Vec3;
use embassy_ssd1306_3d::{Mesh, Edge};

// Tetrahedron: 4 vertices, 6 edges
let tetra: Mesh<4, 6> = Mesh {
    vertices: [
        Vec3::new( 0.0,  20.0,  0.0),
        Vec3::new(-18.0, -10.0, -10.0),
        Vec3::new( 18.0, -10.0, -10.0),
        Vec3::new(  0.0, -10.0,  18.0),
    ],
    edges: [
        Edge(0,1), Edge(0,2), Edge(0,3),
        Edge(1,2), Edge(2,3), Edge(3,1),
    ],
};
```

---

## Complete example: multi-object rotation

```rust
#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::i2c::{Config as I2cConfig, I2c, InterruptHandler as I2cIrqs};
use embassy_rp::peripherals::I2C0;
use embassy_time::Timer;
use embassy_ssd1306::Ssd1306;
use embassy_ssd1306_graphics::Graphics;
use embassy_ssd1306_3d::{cube, pyramid, octahedron, icosahedron, rotation_xyz, Camera};
use embedded_trig_f32::consts::TAU;
use {defmt_rtt as _, panic_probe as _};
use rp2350_linker as _;

bind_interrupts!(struct Irqs { I2C0_IRQ => I2cIrqs<I2C0>; });

#[unsafe(export_name = "_defmt_panic")]
fn forced_defmt_panic() -> ! { panic_probe::hard_fault() }

const FRAME_MS:         u64 = 50;
const FRAMES_PER_SCENE: u32 = 3_000 / 50;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let mut cfg = I2cConfig::default();
    cfg.frequency = 400_000;
    let i2c = I2c::new_async(p.I2C0, p.PIN_5, p.PIN_4, Irqs, cfg);

    let mut oled = Ssd1306::new(i2c, 0x3C);
    oled.init().await.unwrap();

    let cam   = Camera::new(64.0, 32.0, 80.0);
    let cube_m = cube(18.0);
    let pyr_m  = pyramid(14.0, 28.0);
    let octa_m = octahedron(20.0);
    let ico_m  = icosahedron(16.0);

    let mut angle: f32 = 0.0;
    let mut frame: u32 = 0;

    loop {
        let scene = (frame / FRAMES_PER_SCENE) % 4;

        oled.clear();
        oled.draw_rect(0, 0, 128, 64, true);

        {
            let mut gfx = Graphics::new(&mut oled);
            let rot = rotation_xyz(angle * 0.7, angle, angle * 0.4);

            match scene {
                0 => cube_m.draw_wireframe(&mut gfx, &cam, &rot),
                1 => pyr_m.draw_wireframe(&mut gfx, &cam, &rot),
                2 => octa_m.draw_wireframe(&mut gfx, &cam, &rot),
                _ => ico_m.draw_wireframe(&mut gfx, &cam, &rot),
            }
        }

        oled.flush().await.unwrap();

        angle += 0.05;
        if angle > TAU { angle -= TAU; }
        frame = frame.wrapping_add(1);
        Timer::after_millis(FRAME_MS).await;
    }
}
```

---

## Internal architecture

```text
draw_wireframe()
    │
    ├── transform(v, rot)        embedded-vector × embedded-matrix
    │       └── Matrix3x3 · Vec3
    │
    ├── cam.project(v_world)     perspective projection f32 → (i32, i32)
    │       └── None if Z + focal ≤ 0 (camera clipping)
    │
    └── line(gfx, x0, y0, x1, y1, true)
            └── embassy-ssd1306-graphics (integer-only Bresenham)
                    └── oled.draw_pixel()
                            └── SSD1306 framebuffer
```

---

## License

GPL-2.0-or-later — Jorge Andre Castro
