use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

use header_parsing::parse_header;
use simple_vectors::Vector;

type Vec3 = Vector<f32, 3>;

fn length(v: Vec3) -> f32 {
    let [x, y, z]: [f32; 3] = v.into();
    (x * x + y * y + z * z).sqrt()
}

fn normalize(v: Vec3) -> Vec3 {
    let len = length(v);
    if len > 1e-6 { v / len } else { v }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Additive,
    Alpha,
}

#[derive(Debug, Clone, Copy)]
pub enum EmitterShape {
    Point,
    Sphere { radius: f32 },
    Hemisphere { radius: f32 },
    Cone { radius: f32, angle: f32 },
    Box { size: [f32; 3] },
    Disc { radius: f32 },
    Ring { radius: f32 },
    Line { length: f32 },
    Rect { size: [f32; 2] },
}

#[derive(Debug, Clone)]
pub struct Curve {
    keys: Vec<[f32; 2]>,
}

impl Curve {
    fn constant(value: f32) -> Self {
        Self {
            keys: vec![[0.0, value]],
        }
    }

    pub fn linear(start: f32, end: f32) -> Self {
        Self {
            keys: vec![[0.0, start], [1.0, end]],
        }
    }

    pub fn from_keys(mut keys: Vec<[f32; 2]>) -> Self {
        keys.sort_by(|a, b| a[0].total_cmp(&b[0]));
        if keys.is_empty() {
            Self::constant(1.0)
        } else {
            Self { keys }
        }
    }

    pub fn keys(&self) -> &[[f32; 2]] {
        &self.keys
    }

    fn parse(value: &str) -> Self {
        let mut keys: Vec<[f32; 2]> = value
            .split_whitespace()
            .filter_map(|pair| {
                let (time, amount) = pair.split_once(':')?;
                Some([time.parse().ok()?, amount.parse().ok()?])
            })
            .collect();
        keys.sort_by(|a, b| a[0].total_cmp(&b[0]));
        if keys.is_empty() {
            Self::constant(1.0)
        } else {
            Self { keys }
        }
    }

    pub fn sample(&self, time: f32) -> f32 {
        let &[first_time, first_value] = &self.keys[0];
        if time <= first_time {
            return first_value;
        }
        let &[last_time, last_value] = &self.keys[self.keys.len() - 1];
        if time >= last_time {
            return last_value;
        }
        for window in self.keys.windows(2) {
            let &[left_time, left_value] = &window[0];
            let &[right_time, right_value] = &window[1];
            if time <= right_time {
                let span = right_time - left_time;
                let blend = if span > 1e-6 {
                    (time - left_time) / span
                } else {
                    0.0
                };
                return left_value + (right_value - left_value) * blend;
            }
        }
        last_value
    }
}

#[derive(Debug, Clone)]
pub struct Gradient {
    keys: Vec<(f32, [f32; 4])>,
}

impl Gradient {
    pub fn linear(start: [f32; 4], end: [f32; 4]) -> Self {
        Self {
            keys: vec![(0.0, start), (1.0, end)],
        }
    }

    pub fn from_stops(mut keys: Vec<(f32, [f32; 4])>) -> Self {
        keys.sort_by(|a, b| a.0.total_cmp(&b.0));
        if keys.is_empty() {
            Self::constant([1.0, 1.0, 1.0, 1.0])
        } else {
            Self { keys }
        }
    }

    pub fn stops(&self) -> &[(f32, [f32; 4])] {
        &self.keys
    }

    fn constant(color: [f32; 4]) -> Self {
        Self {
            keys: vec![(0.0, color)],
        }
    }

    fn parse(value: &str) -> Self {
        let mut keys: Vec<(f32, [f32; 4])> = value
            .split_whitespace()
            .filter_map(|pair| {
                let (time, color) = pair.split_once(':')?;
                Some((time.parse().ok()?, parse_hex_color(color)?))
            })
            .collect();
        keys.sort_by(|a, b| a.0.total_cmp(&b.0));
        if keys.is_empty() {
            Self::constant([1.0, 1.0, 1.0, 1.0])
        } else {
            Self { keys }
        }
    }

    pub fn sample(&self, time: f32) -> [f32; 4] {
        let (first_time, first_color) = self.keys[0];
        if time <= first_time {
            return first_color;
        }
        let (last_time, last_color) = self.keys[self.keys.len() - 1];
        if time >= last_time {
            return last_color;
        }
        for window in self.keys.windows(2) {
            let (left_time, left_color) = window[0];
            let (right_time, right_color) = window[1];
            if time <= right_time {
                let span = right_time - left_time;
                let blend = if span > 1e-6 {
                    (time - left_time) / span
                } else {
                    0.0
                };
                let mut result = [0.0; 4];
                for channel in 0..4 {
                    result[channel] =
                        left_color[channel] + (right_color[channel] - left_color[channel]) * blend;
                }
                return result;
            }
        }
        last_color
    }
}

fn parse_hex_color(text: &str) -> Option<[f32; 4]> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let component = |start: usize| u8::from_str_radix(&hex[start..start + 2], 16).ok();
    let red = component(0)?;
    let green = component(2)?;
    let blue = component(4)?;
    let alpha = if hex.len() == 8 { component(6)? } else { 255 };
    Some([
        f32::from(red) / 255.0,
        f32::from(green) / 255.0,
        f32::from(blue) / 255.0,
        f32::from(alpha) / 255.0,
    ])
}

#[derive(Debug, Clone)]
pub struct ParticleEffect {
    pub texture: String,
    pub sheet: [u32; 2],
    pub blend: BlendMode,
    pub max_particles: usize,
    pub looping: bool,
    pub duration: f32,
    pub rate: f32,
    pub rate_distance: f32,
    pub burst: u32,
    pub shape: EmitterShape,
    pub lifetime: [f32; 2],
    pub speed: [f32; 2],
    pub gravity: Vec3,
    pub drag: f32,
    pub size: Curve,
    pub color: Gradient,
    pub spin: [f32; 2],
}

impl ParticleEffect {
    #[allow(clippy::expect_used)]
    pub fn load(path: &Path) -> Self {
        let content = std::fs::read_to_string(path).expect("failed to read .particle file");
        Self::parse(&content)
    }

    pub fn reload_if_changed(path: &Path, last_modified: &mut Option<SystemTime>) -> Option<Self> {
        let modified = std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()?;
        if *last_modified == Some(modified) {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        *last_modified = Some(modified);
        Some(Self::parse(&content))
    }

    pub fn parse(content: &str) -> Self {
        let mut path = Vec::new();
        let mut effect = Self {
            texture: String::new(),
            sheet: [1, 1],
            blend: BlendMode::Alpha,
            max_particles: 256,
            looping: true,
            duration: 1.0,
            rate: 0.0,
            rate_distance: 0.0,
            burst: 0,
            shape: EmitterShape::Point,
            lifetime: [1.0, 1.0],
            speed: [0.0, 0.0],
            gravity: Vec3::default(),
            drag: 0.0,
            size: Curve::constant(0.2),
            color: Gradient::constant([1.0, 1.0, 1.0, 1.0]),
            spin: [0.0, 0.0],
        };

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(header_result) = parse_header(&mut path, line) {
                if let Ok(changes) = header_result {
                    changes.apply();
                }
                continue;
            }
            let Some((key, value)) = line.split_once(' ') else {
                continue;
            };
            let value = value.trim();
            let section = path.first().map_or("", |name| &**name);
            apply_field(&mut effect, section, key.trim(), value);
        }

        effect
    }
}

impl std::str::FromStr for ParticleEffect {
    type Err = std::convert::Infallible;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(text))
    }
}

impl std::fmt::Display for Curve {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, &[time, value]) in self.keys.iter().enumerate() {
            if index > 0 {
                write!(formatter, " ")?;
            }
            write!(formatter, "{time}:{value}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Gradient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, (time, color)) in self.keys.iter().enumerate() {
            if index > 0 {
                write!(formatter, " ")?;
            }
            write!(formatter, "{time}:{}", color_to_hex(*color))?;
        }
        Ok(())
    }
}

impl std::fmt::Display for ParticleEffect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let blend = match self.blend {
            BlendMode::Additive => "additive",
            BlendMode::Alpha => "alpha",
        };
        writeln!(formatter, "texture {}", self.texture)?;
        if self.sheet != [1, 1] {
            writeln!(formatter, "sheet {} {}", self.sheet[0], self.sheet[1])?;
        }
        writeln!(formatter, "blend {blend}")?;
        writeln!(formatter, "max_particles {}", self.max_particles)?;
        writeln!(formatter, "loop {}", self.looping)?;
        writeln!(formatter, "duration {}", self.duration)?;

        writeln!(formatter, "\n# emit")?;
        writeln!(formatter, "rate {}", self.rate)?;
        if self.rate_distance > 0.0 {
            writeln!(formatter, "rate_distance {}", self.rate_distance)?;
        }
        if self.burst > 0 {
            writeln!(formatter, "burst {}", self.burst)?;
        }
        match self.shape {
            EmitterShape::Point => writeln!(formatter, "shape point")?,
            EmitterShape::Sphere { radius } => {
                writeln!(formatter, "shape sphere")?;
                writeln!(formatter, "radius {radius}")?;
            }
            EmitterShape::Cone { radius, angle } => {
                writeln!(formatter, "shape cone")?;
                writeln!(formatter, "radius {radius}")?;
                writeln!(formatter, "angle {angle}")?;
            }
            EmitterShape::Box { size: [x, y, z] } => {
                writeln!(formatter, "shape box")?;
                writeln!(formatter, "box_size {x} {y} {z}")?;
            }
            EmitterShape::Hemisphere { radius } => {
                writeln!(formatter, "shape hemisphere")?;
                writeln!(formatter, "radius {radius}")?;
            }
            EmitterShape::Disc { radius } => {
                writeln!(formatter, "shape disc")?;
                writeln!(formatter, "radius {radius}")?;
            }
            EmitterShape::Ring { radius } => {
                writeln!(formatter, "shape ring")?;
                writeln!(formatter, "radius {radius}")?;
            }
            EmitterShape::Line { length } => {
                writeln!(formatter, "shape line")?;
                writeln!(formatter, "length {length}")?;
            }
            EmitterShape::Rect { size: [x, z] } => {
                writeln!(formatter, "shape rect")?;
                writeln!(formatter, "rect_size {x} {z}")?;
            }
        }

        let [gravity_x, gravity_y, gravity_z]: [f32; 3] = self.gravity.into();
        writeln!(formatter, "\n# life")?;
        writeln!(
            formatter,
            "lifetime {} {}",
            self.lifetime[0], self.lifetime[1]
        )?;
        writeln!(formatter, "speed {} {}", self.speed[0], self.speed[1])?;
        writeln!(formatter, "gravity {gravity_x} {gravity_y} {gravity_z}")?;
        writeln!(formatter, "drag {}", self.drag)?;

        writeln!(formatter, "\n# size")?;
        writeln!(formatter, "curve {}", self.size)?;

        writeln!(formatter, "\n# color")?;
        writeln!(formatter, "gradient {}", self.color)?;

        writeln!(formatter, "\n# spin")?;
        writeln!(formatter, "rate {} {}", self.spin[0], self.spin[1])?;
        Ok(())
    }
}

fn color_to_hex(color: [f32; 4]) -> String {
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        byte(color[0]),
        byte(color[1]),
        byte(color[2]),
        byte(color[3])
    )
}

fn apply_field(effect: &mut ParticleEffect, section: &str, key: &str, value: &str) {
    match (section, key) {
        ("", "texture") => effect.texture = value.to_string(),
        ("", "sheet") => {
            let mut parts = value.split_whitespace();
            let columns = parts.next().and_then(|part| part.parse().ok()).unwrap_or(1);
            let rows = parts.next().and_then(|part| part.parse().ok()).unwrap_or(1);
            effect.sheet = [columns.max(1), rows.max(1)];
        }
        ("", "blend") => {
            effect.blend = if value == "additive" {
                BlendMode::Additive
            } else {
                BlendMode::Alpha
            };
        }
        ("", "max_particles") => effect.max_particles = value.parse().unwrap_or(256),
        ("", "loop") => effect.looping = value == "true",
        ("", "duration") => effect.duration = value.parse().unwrap_or(1.0),
        ("emit", "rate") => effect.rate = value.parse().unwrap_or(0.0),
        ("emit", "rate_distance") => effect.rate_distance = value.parse().unwrap_or(0.0),
        ("emit", "burst") => effect.burst = value.parse().unwrap_or(0),
        ("emit", "shape") => effect.shape = parse_shape(value),
        ("emit", "radius") => set_shape_radius(&mut effect.shape, value.parse().unwrap_or(0.0)),
        ("emit", "angle") => set_shape_angle(&mut effect.shape, value.parse().unwrap_or(0.0)),
        ("emit", "box_size") => {
            effect.shape = EmitterShape::Box {
                size: parse_vec3_array(value),
            }
        }
        ("emit", "length") => {
            effect.shape = EmitterShape::Line {
                length: value.parse().unwrap_or(0.0),
            }
        }
        ("emit", "rect_size") => {
            let array = parse_vec3_array(value);
            effect.shape = EmitterShape::Rect {
                size: [array[0], array[1]],
            }
        }
        ("life", "lifetime") => effect.lifetime = parse_range(value),
        ("life", "speed") => effect.speed = parse_range(value),
        ("life", "gravity") => effect.gravity = Vec3::from(parse_vec3_array(value)),
        ("life", "drag") => effect.drag = value.parse().unwrap_or(0.0),
        ("size", "curve") => effect.size = Curve::parse(value),
        ("color", "gradient") => effect.color = Gradient::parse(value),
        ("spin", "rate") => effect.spin = parse_range(value),
        _ => {}
    }
}

fn parse_shape(value: &str) -> EmitterShape {
    match value {
        "sphere" => EmitterShape::Sphere { radius: 0.0 },
        "hemisphere" => EmitterShape::Hemisphere { radius: 0.0 },
        "cone" => EmitterShape::Cone {
            radius: 0.0,
            angle: 25.0,
        },
        "box" => EmitterShape::Box { size: [0.0; 3] },
        "disc" => EmitterShape::Disc { radius: 0.0 },
        "ring" => EmitterShape::Ring { radius: 0.0 },
        "line" => EmitterShape::Line { length: 0.0 },
        "rect" => EmitterShape::Rect { size: [0.0; 2] },
        _ => EmitterShape::Point,
    }
}

fn set_shape_radius(shape: &mut EmitterShape, value: f32) {
    match shape {
        EmitterShape::Sphere { radius }
        | EmitterShape::Hemisphere { radius }
        | EmitterShape::Cone { radius, .. }
        | EmitterShape::Disc { radius }
        | EmitterShape::Ring { radius } => *radius = value,
        _ => {}
    }
}

fn set_shape_angle(shape: &mut EmitterShape, value: f32) {
    if let EmitterShape::Cone { angle, .. } = shape {
        *angle = value;
    }
}

fn parse_range(value: &str) -> [f32; 2] {
    let mut parts = value.split_whitespace();
    let min: f32 = parts
        .next()
        .and_then(|text| text.parse().ok())
        .unwrap_or(0.0);
    let max: f32 = parts
        .next()
        .and_then(|text| text.parse().ok())
        .unwrap_or(min);
    [min, max]
}

fn parse_vec3_array(value: &str) -> [f32; 3] {
    let mut result = [0.0; 3];
    for (slot, text) in result.iter_mut().zip(value.split_whitespace()) {
        *slot = text.parse().unwrap_or(0.0);
    }
    result
}

struct Rng {
    state: u32,
}

impl Rng {
    fn new(seed: u32) -> Self {
        Self { state: seed | 1 }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }

    fn range(&mut self, bounds: [f32; 2]) -> f32 {
        let [min, max] = bounds;
        min + (max - min) * self.unit()
    }

    fn direction(&mut self) -> Vec3 {
        let z = self.unit() * 2.0 - 1.0;
        let azimuth = self.unit() * std::f32::consts::TAU;
        let radius = (1.0 - z * z).max(0.0).sqrt();
        Vec3::from([radius * azimuth.cos(), z, radius * azimuth.sin()])
    }
}

struct Particle {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    lifetime: f32,
    rotation: f32,
    spin: f32,
}

pub struct ParticleRender {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub size: f32,
    pub rotation: f32,
    pub life: f32,
}

pub struct Emitter {
    particles: Vec<Particle>,
    spawn_accumulator: f32,
    distance_accumulator: f32,
    age: f32,
    emitting: bool,
    rng: Rng,
}

impl Emitter {
    pub fn new(seed: u32) -> Self {
        Self {
            particles: Vec::new(),
            spawn_accumulator: 0.0,
            distance_accumulator: 0.0,
            age: 0.0,
            emitting: true,
            rng: Rng::new(seed),
        }
    }

    pub fn finished(&self) -> bool {
        !self.emitting && self.particles.is_empty()
    }

    pub fn stop(&mut self) {
        self.emitting = false;
    }

    pub fn update(
        &mut self,
        effect: &ParticleEffect,
        delta_time: f32,
        origin: Vec3,
        velocity: Vec3,
        wind: Vec3,
    ) {
        let was_fresh = self.age == 0.0;
        self.age += delta_time;

        if self.emitting {
            if was_fresh {
                for _ in 0..effect.burst {
                    self.spawn(effect, origin);
                }
            }
            if effect.rate > 0.0 {
                self.spawn_accumulator += effect.rate * delta_time;
                while self.spawn_accumulator >= 1.0 {
                    self.spawn_accumulator -= 1.0;
                    self.spawn(effect, origin);
                }
            }
            if effect.rate_distance > 0.0 {
                self.distance_accumulator += effect.rate_distance * length(velocity) * delta_time;
                while self.distance_accumulator >= 1.0 {
                    self.distance_accumulator -= 1.0;
                    self.spawn(effect, origin);
                }
            }
            if !effect.looping && self.age >= effect.duration {
                self.emitting = false;
            }
        }

        let drag = (1.0 - effect.drag * delta_time).max(0.0);
        let mut index = 0;
        while index < self.particles.len() {
            let particle = &mut self.particles[index];
            particle.age += delta_time;
            if particle.age >= particle.lifetime {
                self.particles.swap_remove(index);
                continue;
            }
            particle.velocity += (effect.gravity + wind) * delta_time;
            particle.velocity *= drag;
            particle.position += particle.velocity * delta_time;
            particle.rotation += particle.spin * delta_time;
            index += 1;
        }
    }

    fn spawn(&mut self, effect: &ParticleEffect, origin: Vec3) {
        if self.particles.len() >= effect.max_particles {
            return;
        }
        let (offset, direction) = self.sample_shape(&effect.shape);
        let speed = self.rng.range(effect.speed);
        self.particles.push(Particle {
            position: origin + offset,
            velocity: direction * speed,
            age: 0.0,
            lifetime: self.rng.range(effect.lifetime).max(0.01),
            rotation: 0.0,
            spin: self.rng.range(effect.spin),
        });
    }

    fn sample_shape(&mut self, shape: &EmitterShape) -> (Vec3, Vec3) {
        let up = Vec3::from([0.0, 1.0, 0.0]);
        match *shape {
            EmitterShape::Point => (Vec3::default(), up),
            EmitterShape::Sphere { radius } => {
                let direction = self.rng.direction();
                (direction * (radius * self.rng.unit()), direction)
            }
            EmitterShape::Cone { radius, angle } => {
                let half = angle.to_radians();
                let polar = half * self.rng.unit().sqrt();
                let azimuth = self.rng.unit() * std::f32::consts::TAU;
                let direction = normalize(Vec3::from([
                    polar.sin() * azimuth.cos(),
                    polar.cos(),
                    polar.sin() * azimuth.sin(),
                ]));
                let disk = radius * self.rng.unit().sqrt();
                let offset = Vec3::from([azimuth.cos() * disk, 0.0, azimuth.sin() * disk]);
                (offset, direction)
            }
            EmitterShape::Box { size } => {
                let [size_x, size_y, size_z] = size;
                let offset = Vec3::from([
                    (self.rng.unit() - 0.5) * size_x,
                    (self.rng.unit() - 0.5) * size_y,
                    (self.rng.unit() - 0.5) * size_z,
                ]);
                (offset, up)
            }
            EmitterShape::Hemisphere { radius } => {
                let mut direction = self.rng.direction();
                direction[1] = direction[1].abs();
                (direction * (radius * self.rng.unit()), direction)
            }
            EmitterShape::Disc { radius } => {
                let azimuth = self.rng.unit() * std::f32::consts::TAU;
                let distance = radius * self.rng.unit().sqrt();
                let offset = Vec3::from([azimuth.cos() * distance, 0.0, azimuth.sin() * distance]);
                (offset, up)
            }
            EmitterShape::Ring { radius } => {
                let azimuth = self.rng.unit() * std::f32::consts::TAU;
                let offset = Vec3::from([azimuth.cos() * radius, 0.0, azimuth.sin() * radius]);
                (offset, up)
            }
            EmitterShape::Line { length } => {
                let offset = Vec3::from([(self.rng.unit() - 0.5) * length, 0.0, 0.0]);
                (offset, up)
            }
            EmitterShape::Rect {
                size: [size_x, size_z],
            } => {
                let offset = Vec3::from([
                    (self.rng.unit() - 0.5) * size_x,
                    0.0,
                    (self.rng.unit() - 0.5) * size_z,
                ]);
                (offset, up)
            }
        }
    }

    pub fn render_into(&self, effect: &ParticleEffect, out: &mut Vec<ParticleRender>) {
        for particle in &self.particles {
            let life = (particle.age / particle.lifetime).clamp(0.0, 1.0);
            out.push(ParticleRender {
                position: particle.position.into(),
                color: effect.color.sample(life),
                size: effect.size.sample(life),
                rotation: particle.rotation,
                life,
            });
        }
    }
}

pub struct EffectDraw {
    pub texture: String,
    pub sheet: [u32; 2],
    pub blend: BlendMode,
    pub instances: Vec<ParticleRender>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmitterId(u64);

struct ActiveEmitter {
    id: EmitterId,
    effect: String,
    emitter: Emitter,
    origin: Vec3,
    velocity: Vec3,
}

pub struct ParticleSystem {
    effects: HashMap<String, ParticleEffect>,
    emitters: Vec<ActiveEmitter>,
    next_seed: u32,
    next_id: u64,
    wind: Vec3,
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSystem {
    pub fn new() -> Self {
        Self {
            effects: HashMap::new(),
            emitters: Vec::new(),
            next_seed: 1,
            next_id: 1,
            wind: Vec3::default(),
        }
    }

    pub fn set_wind(&mut self, wind: [f32; 3]) {
        self.wind = Vec3::from(wind);
    }

    pub fn register(&mut self, name: &str, effect: ParticleEffect) {
        self.effects.insert(name.to_string(), effect);
    }

    pub fn spawn(&mut self, name: &str, origin: [f32; 3]) -> EmitterId {
        let id = EmitterId(self.next_id);
        self.next_id += 1;
        if !self.effects.contains_key(name) {
            return id;
        }
        self.next_seed = self
            .next_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        self.emitters.push(ActiveEmitter {
            id,
            effect: name.to_string(),
            emitter: Emitter::new(self.next_seed),
            origin: Vec3::from(origin),
            velocity: Vec3::default(),
        });
        id
    }

    pub fn set_emitter_motion(&mut self, id: EmitterId, origin: [f32; 3], velocity: [f32; 3]) {
        if let Some(active) = self.emitters.iter_mut().find(|active| active.id == id) {
            active.origin = Vec3::from(origin);
            active.velocity = Vec3::from(velocity);
        }
    }

    pub fn update(&mut self, delta_time: f32) {
        let effects = &self.effects;
        let wind = self.wind;
        self.emitters.retain_mut(|active| {
            let Some(effect) = effects.get(&active.effect) else {
                return false;
            };
            active
                .emitter
                .update(effect, delta_time, active.origin, active.velocity, wind);
            !active.emitter.finished()
        });
    }

    pub fn is_empty(&self) -> bool {
        self.emitters.is_empty()
    }

    pub fn collect(&self) -> Vec<EffectDraw> {
        let mut by_effect: HashMap<&str, Vec<ParticleRender>> = HashMap::new();
        for active in &self.emitters {
            let Some(effect) = self.effects.get(&active.effect) else {
                continue;
            };
            let instances = by_effect.entry(active.effect.as_str()).or_default();
            active.emitter.render_into(effect, instances);
        }
        by_effect
            .into_iter()
            .filter_map(|(name, instances)| {
                let effect = self.effects.get(name)?;
                Some(EffectDraw {
                    texture: effect.texture.clone(),
                    sheet: effect.sheet,
                    blend: effect.blend,
                    instances,
                })
            })
            .collect()
    }
}
