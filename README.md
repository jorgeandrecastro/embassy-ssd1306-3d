# embassy-ssd1306-3d

Moteur 3D filaire `no_std` pour écrans OLED SSD1306 (128×64), construit au-dessus de [`embassy-ssd1306-graphics`](https://github.com/jorgeandrecastro/embassy-ssd1306-graphics).

Zéro allocation · Zéro unsafe · Integer-friendly · testée sur RP2350 / Cortex-M ready

---

## Stack complète

```text
embassy-ssd1306-3d          ← ce crate (3D → 2D)
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
embassy-ssd1306-3d       = "0.1.0"
embassy-ssd1306-graphics = "0.1.0"
embedded-vector          = "0.1.0"
embassy-ssd1306          = "0.6.0"
```

---

## Démarrage rapide

```rust
use embassy_ssd1306_3d::{cube, Camera, rotation_xyz};
use embassy_ssd1306_graphics::Graphics;

let cam  = Camera::new(64.0, 32.0, 80.0);  // cx, cy, focale
let mesh = cube(18.0);                      // demi-côté 18 px

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

## Référence API

### `Camera`

```rust
pub struct Camera { pub cx: f32, pub cy: f32, pub focal: f32 }
```

Centre de projection + distance focale. Plus `focal` est grand, moins la perspective est marquée.

```rust
// Caméra centrée sur l'écran 128×64, focale standard
let cam = Camera::new(64.0, 32.0, 80.0);

// Projection d'un point :  None si derrière la caméra
let screen: Option<(i32, i32)> = cam.project(&v);
```

| `focal` | Effet visuel |
|---------|-------------|
| 40      | Très grand angle, perspective agressive |
| 80      | Standard, équilibré |
| 150     | Quasi-orthographique |

---

### Rotations

```rust
// Rotations élémentaires autour de chaque axe
let rx = rotation_x(angle);
let ry = rotation_y(angle);
let rz = rotation_z(angle);

// Rotation combinée Rx · Ry · Rz (usage le plus courant)
let rot = rotation_xyz(ax, ay, az);
```

Les matrices sont des [`Matrix3x3`](https://docs.rs/embedded-matrix) et se composent avec `*` :

```rust
// Rotation personnalisée : d'abord Y, puis Z
let rot = rotation_y(ay) * rotation_z(az);
```

---

### `transform`

```rust
pub fn transform(v: &Vec3, m: &Matrix3x3) -> Vec3
```

Applique une rotation à un `Vec3`. Fonction libre car `embedded-vector` et `embedded-matrix` ne se connaissent pas (règle orphan Rust).

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

Tailles connues à la compilation — aucun `Vec`, aucun heap.

```rust
// Dessin filaire complet en une ligne
mesh.draw_wireframe(&mut gfx, &cam, &rot);
```

**Maillages fournis :**

| Fonction | Sommets | Arêtes | Description |
|----------|---------|--------|-------------|
| `cube(s)` | 8 | 12 | Cube, demi-côté `s` |
| `pyramid(base, height)` | 5 | 8 | Pyramide à base carrée |
| `octahedron(r)` | 6 | 12 | Octaèdre régulier |
| `icosahedron(r)` | 12 | 30 | Icosaèdre (ratio doré φ) |

```rust
let c = cube(18.0);
let p = pyramid(14.0, 28.0);
let o = octahedron(20.0);
let i = icosahedron(16.0);
```

---

### Maillage personnalisé

```rust
use embedded_vector::Vec3;
use embassy_ssd1306_3d::{Mesh, Edge};

// Tétraèdre : 4 sommets, 6 arêtes
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

## Exemple complet  rotation multi-objets

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

## Architecture interne

```text
draw_wireframe()
    │
    ├── transform(v, rot)        embedded-vector × embedded-matrix
    │       └── Matrix3x3 · Vec3
    │
    ├── cam.project(v_world)     projection perspective f32 → (i32, i32)
    │       └── None si Z + focal ≤ 0  (clipping caméra)
    │
    └── line(gfx, x0, y0, x1, y1, true)
            └── embassy-ssd1306-graphics  (Bresenham integer-only)
                    └── oled.draw_pixel()
                            └── framebuffer SSD1306
```

---

## Licence

GPL-2.0-or-later — Jorge Andre Castro