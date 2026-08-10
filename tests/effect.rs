#![expect(clippy::float_cmp)]

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

#[test]
fn absurd_rate_stays_bounded_by_max_particles() {
    let mut system = ParticleSystem::new();
    system.register(
        "flood",
        ParticleEffect::parse("max_particles 32\n\n# emit\nrate 1e9\n\n# life\nlifetime 5 5\n"),
    );
    system.spawn("flood", [0.0, 0.0, 0.0]);
    system.update(1.0 / 60.0);

    let draws = system.collect();
    assert_eq!(draws[0].instances.len(), 32);
}

#[test]
fn a_stopped_emitter_lets_its_particles_finish_and_then_vanishes() {
    let mut system = ParticleSystem::new();
    system.register(
        "trail",
        ParticleEffect::parse(
            "max_particles 64\nloop true\n\n# emit\nrate 60\n\n# life\nlifetime 0.5 0.5\n",
        ),
    );
    let id = system.spawn("trail", [0.0, 0.0, 0.0]);
    system.update(0.2);
    let live = system.collect()[0].instances.len();
    assert!(live > 0);

    system.stop_emitter(id);
    system.update(0.1);
    assert!(system.collect()[0].instances.len() <= live);
    assert!(!system.is_empty());

    system.update(1.0);
    assert!(system.is_empty());
    assert!(system.collect().is_empty());
}

fn tint_system() -> ParticleSystem {
    let mut system = ParticleSystem::new();
    system.register(
        "white",
        ParticleEffect::parse(
            "max_particles 4\n\n# emit\nburst 1\nrate 0\n\n# life\nlifetime 5 5\n\n# color\ngradient 0:#ffffffff 1:#ffffff00\n",
        ),
    );
    system
}

#[test]
fn a_tint_multiplies_the_gradient() {
    let mut system = tint_system();
    system.spawn_tinted("white", [0.0, 0.0, 0.0], [1.0, 0.5, 0.0, 1.0]);
    system.update(0.01);

    let draws = system.collect();
    let color = draws[0].instances[0].color;
    assert!((color[0] - 1.0).abs() < 1e-3);
    assert!((color[1] - 0.5).abs() < 1e-3);
    assert!(color[2].abs() < 1e-3);
    assert!(color[3] > 0.9);
}

#[test]
fn a_tint_set_later_reaches_live_particles() {
    let mut system = tint_system();
    let id = system.spawn("white", [0.0, 0.0, 0.0]);
    system.update(0.01);
    assert!((system.collect()[0].instances[0].color[1] - 1.0).abs() < 1e-3);

    system.set_emitter_tint(id, [1.0, 0.0, 0.0, 1.0]);
    let color = system.collect()[0].instances[0].color;
    assert!((color[0] - 1.0).abs() < 1e-3);
    assert!(color[1].abs() < 1e-3);
}

#[test]
fn a_tint_keeps_the_gradient_fade() {
    let mut system = tint_system();
    system.spawn_tinted("white", [0.0, 0.0, 0.0], [0.5, 0.5, 0.5, 1.0]);
    system.update(0.01);
    let early = system.collect()[0].instances[0].color;
    system.update(4.0);
    let late = system.collect()[0].instances[0].color;

    assert!((early[0] - 0.5).abs() < 1e-3);
    assert!((late[0] - 0.5).abs() < 1e-3);
    assert!(late[3] < early[3] * 0.5);
}
