# simple-particles

A data-driven CPU particle system with an optional [wgpu](https://wgpu.rs) renderer
and a live [egui](https://github.com/emilk/egui) editor. Built for small games that
need tunable visual effects without the complexity of GPU compute particles.

Three layers, each behind a feature so you only pull in what you use:

- **engine** (default, no rendering dependencies): the `.particle` format parser,
  animation curves and color gradients, the CPU simulation, and a `ParticleSystem`
  that pools live emitters.
- **`render`**: a `ParticleRenderer` with instanced billboard quads and alpha/additive
  blend pipelines.
- **`editor`**: a standalone `particle-editor` binary for tuning effects live.

## The `.particle` format

Effects are plain text, parsed with the [`header-parsing`](https://crates.io/crates/header-parsing)
format: top-level `key value` pairs, then `# section` headers grouping more pairs.
Curves are `time:value` keyframes, gradients are `time:#rrggbbaa` stops.

```
texture spark.png
blend additive
max_particles 200
loop false
duration 0.5

# emit
rate 60
shape cone
radius 0.2
angle 25

# life
lifetime 0.4 0.8
speed 4 6
gravity 0 -9 0
drag 0.1

# size
curve 0:0.1 0.3:0.5 1:0.0

# color
gradient 0:#fff8c0ff 0.5:#ff8800ff 1:#88000000
```

Emitter shapes: `point`, `sphere`, `hemisphere`, `cone`, `box`, `disc`, `ring`,
`line`, `rect`.

## Library use

```rust
use simple_particles::{ParticleEffect, ParticleSystem};

let mut system = ParticleSystem::new();
system.register("spark", ParticleEffect::parse(include_str!("spark.particle")));
system.spawn("spark", [0.0, 0.0, 0.0]);

// each frame:
system.update(delta_time);
let draws = system.collect(); // grouped by effect, ready for a renderer
```

With the `render` feature, feed `draws` to a `ParticleRenderer`:

```rust
renderer.set_camera(&queue, view_projection);
renderer.prepare(&device, &queue, &draws);
// inside a render pass:
renderer.draw(&mut render_pass);
```

## Editor

```sh
cargo run --features editor -- [optional/path.particle]
```

Orbiting live preview on the left-hand controls. Sliders for every field, an emitter-shape
picker, save/load to `.particle` files. A path passed on the command line is opened on
startup, so the binary works as a file association handler.

## Notes for working in this codebase

- Public types round-trip: `ParticleEffect` implements `Display` (writes the `.particle`
  format) and `FromStr` (wraps `parse`). `FromStr` is infallible — unknown keys are
  ignored, missing fields fall back to defaults, so parsing never errors or panics.
- No implicit global state. A `ParticleSystem` owns its effects and emitters; rendering
  is a separate object you drive explicitly.
- The simulation has no rendering dependency — the default build pulls in neither wgpu
  nor egui, so the engine is usable headless (servers, tests, tools).
