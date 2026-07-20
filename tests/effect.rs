use simple_particles::effect::{BlendMode, Emitter, EmitterShape, ParticleEffect, ParticleSystem};

type Vec3 = simple_vectors::Vector<f32, 3>;

const SAMPLE: &str = "texture spark.png\nblend additive\nmax_particles 200\nloop false\nduration 0.5\n\n# emit\nrate 60\nshape cone\nradius 0.2\nangle 25\n\n# life\nlifetime 0.4 0.8\nspeed 4 6\ngravity 0 -9 0\ndrag 0.1\n\n# size\ncurve 0:0.1 0.3:0.5 1:0.0\n\n# color\ngradient 0:#fff8c0ff 0.5:#ff8800ff 1:#88000000\n";

#[test]
fn parses_all_sections() {
    let effect = ParticleEffect::parse(SAMPLE);
    assert_eq!(effect.texture, "spark.png");
    assert_eq!(effect.blend, BlendMode::Additive);
    assert_eq!(effect.max_particles, 200);
    assert!(!effect.looping);
    assert!((effect.rate - 60.0).abs() < 1e-6);
    assert_eq!(effect.lifetime, [0.4, 0.8]);
    assert_eq!(effect.speed, [4.0, 6.0]);
    assert!(matches!(effect.shape, EmitterShape::Cone { .. }));
}

#[test]
fn curve_interpolates_and_clamps() {
    let curve = ParticleEffect::parse("# size\ncurve 0:0.1 0.3:0.5 1:0.0\n").size;
    assert!((curve.sample(-1.0) - 0.1).abs() < 1e-6);
    assert!((curve.sample(0.0) - 0.1).abs() < 1e-6);
    assert!((curve.sample(0.3) - 0.5).abs() < 1e-6);
    assert!((curve.sample(0.15) - 0.3).abs() < 1e-3);
    assert!((curve.sample(2.0) - 0.0).abs() < 1e-6);
}

#[test]
fn gradient_interpolates_rgba() {
    let gradient = ParticleEffect::parse("# color\ngradient 0:#ffffffff 1:#00000000\n").color;
    let mid = gradient.sample(0.5);
    for channel in mid {
        assert!((channel - 0.5).abs() < 1e-2);
    }
}

#[test]
fn emitter_spawns_then_drains() {
    let effect = ParticleEffect::parse(SAMPLE);
    let mut emitter = Emitter::new(12345);
    let origin = Vec3::default();
    for _ in 0..30 {
        emitter.update(
            &effect,
            1.0 / 60.0,
            origin,
            Vec3::default(),
            Vec3::default(),
        );
    }
    let mut render = Vec::new();
    emitter.render_into(&effect, &mut render);
    assert!(!render.is_empty());
    assert!(!emitter.finished());
    for _ in 0..120 {
        emitter.update(
            &effect,
            1.0 / 60.0,
            origin,
            Vec3::default(),
            Vec3::default(),
        );
    }
    assert!(emitter.finished());
}

#[test]
fn system_spawns_collects_and_drains() {
    let mut system = ParticleSystem::new();
    system.register("spark", ParticleEffect::parse(SAMPLE));
    system.spawn("spark", [1.0, 2.0, 3.0]);
    system.spawn("unknown", [0.0, 0.0, 0.0]);

    for _ in 0..30 {
        system.update(1.0 / 60.0);
    }
    let draws = system.collect();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].texture, "spark.png");
    assert_eq!(draws[0].blend, BlendMode::Additive);
    assert!(!draws[0].instances.is_empty());

    for _ in 0..120 {
        system.update(1.0 / 60.0);
    }
    assert!(system.collect().is_empty());
}

#[test]
fn display_round_trips_through_parse() {
    let original = ParticleEffect::parse(SAMPLE);
    let reparsed = ParticleEffect::parse(&original.to_string());
    assert_eq!(reparsed.texture, original.texture);
    assert_eq!(reparsed.blend, original.blend);
    assert_eq!(reparsed.max_particles, original.max_particles);
    assert_eq!(reparsed.looping, original.looping);
    assert_eq!(reparsed.lifetime, original.lifetime);
    assert_eq!(reparsed.speed, original.speed);
    assert_eq!(reparsed.spin, original.spin);
    assert!((reparsed.size.sample(0.3) - original.size.sample(0.3)).abs() < 1e-3);
    let original_color = original.color.sample(0.5);
    let reparsed_color = reparsed.color.sample(0.5);
    for channel in 0..4 {
        assert!((reparsed_color[channel] - original_color[channel]).abs() < 0.01);
    }
    assert!(matches!(reparsed.shape, EmitterShape::Cone { .. }));
}

#[test]
fn every_shape_round_trips() {
    let shapes = [
        EmitterShape::Point,
        EmitterShape::Sphere { radius: 1.5 },
        EmitterShape::Hemisphere { radius: 0.7 },
        EmitterShape::Cone {
            radius: 0.3,
            angle: 30.0,
        },
        EmitterShape::Box {
            size: [1.0, 2.0, 3.0],
        },
        EmitterShape::Disc { radius: 2.0 },
        EmitterShape::Ring { radius: 1.25 },
        EmitterShape::Line { length: 4.0 },
        EmitterShape::Rect { size: [3.0, 5.0] },
    ];
    for shape in shapes {
        let mut effect = ParticleEffect::parse("");
        effect.shape = shape;
        let reparsed = ParticleEffect::parse(&effect.to_string());
        assert_eq!(
            std::mem::discriminant(&reparsed.shape),
            std::mem::discriminant(&shape),
            "shape did not round-trip: {shape:?}"
        );
    }
}

#[test]
fn moving_emitter_emits_only_with_velocity() {
    let mut effect = ParticleEffect::parse("");
    effect.rate_distance = 10.0;
    effect.lifetime = [5.0, 5.0];
    let mut system = ParticleSystem::new();
    system.register("trail", effect);
    let id = system.spawn("trail", [0.0, 0.0, 0.0]);

    for _ in 0..20 {
        system.update(1.0 / 60.0);
    }
    let resting = system.collect();
    assert!(resting.iter().all(|draw| draw.instances.is_empty()));

    for _ in 0..30 {
        system.set_emitter_motion(id, [0.0, 0.0, 0.0], [5.0, 0.0, 0.0]);
        system.update(1.0 / 60.0);
    }
    let moving = system.collect();
    assert!(moving.iter().any(|draw| !draw.instances.is_empty()));
}

#[test]
fn rate_distance_round_trips() {
    let mut effect = ParticleEffect::parse("");
    effect.rate_distance = 12.5;
    let reparsed = ParticleEffect::parse(&effect.to_string());
    assert!((reparsed.rate_distance - 12.5).abs() < 1e-4);
}

#[test]
fn sheet_round_trips() {
    let mut effect = ParticleEffect::parse("");
    assert_eq!(effect.sheet, [1, 1]);
    effect.sheet = [4, 2];
    let reparsed = ParticleEffect::parse(&effect.to_string());
    assert_eq!(reparsed.sheet, [4, 2]);
}
