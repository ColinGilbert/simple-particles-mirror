# simple-particles

A data-driven CPU particle system with an optional [wgpu](https://wgpu.rs) renderer
and a live [egui](https://github.com/emilk/egui) editor. Built for small games that
need tunable visual effects without the complexity of GPU compute particles.

Three layers, so you only pull in what you use:

- **engine** (default, no rendering dependencies): the `.particle` format parser,
  animation curves and color gradients, the CPU simulation, and a `ParticleSystem`
  that pools live emitters.
- **`render`** (feature): a `ParticleRenderer` with instanced billboard quads and
  alpha/additive blend pipelines.
- **`document`** (feature): a `document_state::FileFormat` implementation for
  `ParticleEffect`, so an editor can drive it through a `Document` with undo,
  dirty tracking and atomic saves.
- **`simple-particles-editor`** (separate crate in this repo): the live editor, so
  the library itself never pulls in a UI stack. Runs as the standalone
  `particle-editor` binary, or embeds into a host window as an `editor-panels`
  panel.

## The `.particle` format

Effects are plain text, parsed with the [`header-parsing`](https://crates.io/crates/header-parsing)
format: top-level `key value` pairs, then `# section` headers grouping more pairs.
Curves are `time:value` keyframes, gradients are `time:#rrggbbaa` stops.

```
texture spark.png
sheet 4 4
blend additive
max_particles 200
loop false
duration 0.5

# emit
rate 60
rate_distance 0
burst 0
shape cone
radius 0.2
angle 25

# life
lifetime 0.4 0.8
speed 4 6
gravity 0 -9 0
drag 0.1
spin -2 2

# size
curve 0:0.1 0.3:0.5 1:0.0

# color
gradient 0:#fff8c0ff 0.5:#ff8800ff 1:#88000000
```

Emitter shapes: `point`, `sphere`, `hemisphere`, `cone`, `box`, `disc`, `ring`,
`line`, `rect`. Shape parameters follow the `shape` line: `radius` (sphere, hemisphere,
cone, disc, ring), `angle` (cone), `box_size x y z` (box), `rect_size x z` (rect),
`length` (line).

`rate` emits per second; `rate_distance` adds emission per world unit the emitter
travels (for trails); `burst` emits a one-off count on spawn. `spin min max` randomizes
angular velocity.

### Textures and sprite sheets

`texture <name>` selects a registered texture (default `circle`, a built-in soft dot).
`sheet <columns> <rows>` treats that texture as a sprite sheet and plays one frame per
cell across the particle's lifetime — top-left to bottom-right, exactly once. Omit
`sheet` (or set `1 1`) for a static texture.

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

Register textures by name before rendering. The renderer builds the GPU texture from
raw RGBA bytes — it does no image decoding itself, so you keep your own decoder and it
stays wasm-friendly:

```rust
renderer.register_texture(&device, &queue, "spark", &rgba, width, height);
```

An effect's `texture` field then selects it by name; unknown names fall back to the
built-in `circle`.

### Soft particles

Pass the scene depth buffer to fade particles out where they intersect geometry, instead
of showing a hard slice through it. The depth attachment must be bound read-only in the
pass that draws the particles:

```rust
renderer.set_soft_particles(&device, &queue, Some(&depth_view), near, far, 0.5);
```

The last argument is the fade distance in world units — the knob to tune per scene.
Pass `None` to turn the fade back off.

### Hot reload

Re-parse an effect when its file changes on disk, for live tuning without a restart. You
keep the timestamp, so there is no background thread and no file-watching dependency:

```rust
let mut last_modified = None;
if let Some(effect) = ParticleEffect::reload_if_changed(path, &mut last_modified) {
    system.register("spark", effect);
}
```

### Curves and gradients

`Curve` and `Gradient` hold arbitrarily many keys, not just endpoints. Read them with
`keys()` / `stops()` and rebuild with `Curve::from_keys` / `Gradient::from_stops`, which
sort the keys and never leave the set empty.

## Editor

```sh
cargo run -p simple-particles-editor -- [optional/path.particle]
```

Drag the preview to orbit (with a Blender-style axis gizmo for orientation), or pick a
motion path to watch a moving emitter. Sliders for every field, an emitter-shape picker,
a texture and sprite-sheet selector, multi-point editors for the size curve and the color
gradient, and save/load to `.particle` files. Wind shows up as a faint noise backdrop
scrolling in its direction. A path passed on the command line is opened on startup, so
the binary works as a file association handler, and edits made to that file elsewhere are
picked up while the editor runs.

The interface ships in English and German and follows the system language (`LANG`,
`LC_ALL`, `LC_MESSAGES`, `LANGUAGE`), with a dropdown to override it. Translations live
in `assets/lang/<code>.lang` as `key value` lines; adding a language means dropping in a
file and listing it in `Language`.

## Notes for working in this codebase

- Public types round-trip: `ParticleEffect` implements `Display` (writes the `.particle`
  format) and `FromStr` (wraps `parse`). `FromStr` is infallible — unknown keys are
  ignored, missing fields fall back to defaults, so parsing never errors or panics.
- No implicit global state. A `ParticleSystem` owns its effects and emitters; rendering
  is a separate object you drive explicitly.
- The simulation has no rendering dependency — the default build pulls in neither wgpu
  nor egui, so the engine is usable headless (servers, tests, tools).
