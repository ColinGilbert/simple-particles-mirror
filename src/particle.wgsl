struct Camera {
    view_projection: mat4x4<f32>,
    near: f32,
    far: f32,
    fade_distance: f32,
    soft_enabled: f32,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(2) @binding(0)
var scene_depth: texture_2d<f32>;

fn view_distance(depth: f32) -> f32 {
    return camera.near * camera.far
        / (camera.far - depth * (camera.far - camera.near));
}

@group(1) @binding(0)
var particle_texture: texture_2d<f32>;
@group(1) @binding(1)
var particle_sampler: sampler;

struct Instance {
    @location(0) position: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) rotation: f32,
    @location(4) life: f32,
    @location(5) sheet: vec2<f32>,
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

    let columns = max(instance.sheet.x, 1.0);
    let rows = max(instance.sheet.y, 1.0);
    let frame_count = columns * rows;
    let frame = floor(min(instance.life, 0.999) * frame_count);
    let column = frame - floor(frame / columns) * columns;
    let row = floor(frame / columns);
    let cell = corner + vec2<f32>(0.5, 0.5);
    let sheet_uv = vec2<f32>(
        (column + cell.x) / columns,
        (row + 1.0 - cell.y) / rows,
    );

    var output: VertexOutput;
    output.clip_position = camera.view_projection * vec4<f32>(world_pos, 1.0);
    output.uv = sheet_uv;
    output.color = instance.color;
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(particle_texture, particle_sampler, input.uv);
    var color = texel * input.color;
    if camera.soft_enabled > 0.5 {
        let scene = textureLoad(scene_depth, vec2<i32>(input.clip_position.xy), 0).r;
        let gap = view_distance(scene) - view_distance(input.clip_position.z);
        color.a *= clamp(gap / camera.fade_distance, 0.0, 1.0);
    }
    return color;
}
