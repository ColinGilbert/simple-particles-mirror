struct Camera {
    view_projection: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var particle_texture: texture_2d<f32>;
@group(1) @binding(1)
var particle_sampler: sampler;

struct Instance {
    @location(0) position: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) rotation: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32, instance: Instance) -> VertexOutput {
    let corners = array<vec2<f32>, 4>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>(-0.5,  0.5),
        vec2<f32>( 0.5,  0.5),
    );
    let indices = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let corner = corners[indices[vertex_index]];

    let spin_sin = sin(instance.rotation);
    let spin_cos = cos(instance.rotation);
    let rotated = vec2<f32>(
        corner.x * spin_cos - corner.y * spin_sin,
        corner.x * spin_sin + corner.y * spin_cos,
    );

    let right = normalize(vec3<f32>(
        camera.view_projection[0][0],
        camera.view_projection[1][0],
        camera.view_projection[2][0],
    ));
    let up = normalize(vec3<f32>(
        camera.view_projection[0][1],
        camera.view_projection[1][1],
        camera.view_projection[2][1],
    ));

    let world_pos = instance.position
        + right * rotated.x * instance.size
        + up * rotated.y * instance.size;

    var output: VertexOutput;
    output.clip_position = camera.view_projection * vec4<f32>(world_pos, 1.0);
    output.uv = corner + vec2<f32>(0.5, 0.5);
    output.color = instance.color;
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(particle_texture, particle_sampler, input.uv);
    return texel * input.color;
}
