use naga::{
    ShaderStage,
    back::glsl::{Options, PipelineOptions, Version, Writer, WriterFlags},
    front::wgsl,
    proc::BoundsCheckPolicies,
    valid::{Capabilities, ValidationFlags, Validator},
};

const PARTICLE_SHADER: &str = include_str!("../src/particle.wgsl");

fn translate_to_webgl2(entry_point: &str, shader_stage: ShaderStage) -> Result<String, String> {
    let module = wgsl::parse_str(PARTICLE_SHADER).map_err(|error| error.to_string())?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|error| error.to_string())?;
    let options = Options {
        version: Version::new_gles(300),
        writer_flags: WriterFlags::ADJUST_COORDINATE_SPACE,
        ..Options::default()
    };
    let pipeline_options = PipelineOptions {
        shader_stage,
        entry_point: entry_point.to_string(),
        multiview: None,
    };
    let mut output = String::new();
    let mut writer = Writer::new(
        &mut output,
        &module,
        &info,
        &options,
        &pipeline_options,
        BoundsCheckPolicies::default(),
    )
    .map_err(|error| error.to_string())?;
    writer.write().map_err(|error| error.to_string())?;
    Ok(output)
}

#[test]
fn particle_fragment_translates_to_webgl2_glsl() {
    let result = translate_to_webgl2("fragment_main", ShaderStage::Fragment);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn particle_vertex_translates_to_webgl2_glsl() {
    let result = translate_to_webgl2("vertex_main", ShaderStage::Vertex);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn particle_fragment_still_reads_scene_depth() {
    let glsl = translate_to_webgl2("fragment_main", ShaderStage::Fragment).unwrap_or_default();
    assert!(glsl.contains("texelFetch"), "{glsl}");
}
