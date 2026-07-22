#![expect(clippy::unwrap_used)]

use std::fs::write;

use simple_particles::ParticleEffect;

#[test]
fn reloads_only_after_the_file_changes() {
    let path = std::env::temp_dir().join("simple-particles-hot-reload.particle");
    write(&path, "texture first\n").unwrap();

    let mut last_modified = None;
    let first = ParticleEffect::reload_if_changed(&path, &mut last_modified);
    assert_eq!(
        first.map(|effect| effect.texture),
        Some("first".to_string())
    );

    assert!(ParticleEffect::reload_if_changed(&path, &mut last_modified).is_none());

    let stamp = last_modified.unwrap() + std::time::Duration::from_secs(1);
    write(&path, "texture second\n").unwrap();
    std::fs::File::open(&path)
        .unwrap()
        .set_modified(stamp)
        .unwrap();

    let second = ParticleEffect::reload_if_changed(&path, &mut last_modified);
    assert_eq!(
        second.map(|effect| effect.texture),
        Some("second".to_string())
    );

    std::fs::remove_file(&path).unwrap();
}
