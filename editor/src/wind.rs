const SHADER: &str = "
struct Uniform {
    offset: vec2<f32>,
    scale: vec2<f32>,
    strength: f32,
};
@group(0) @binding(0) var<uniform> info: Uniform;

var<private> CORNERS: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(3.0, -1.0),
    vec2<f32>(-1.0, 3.0),
);

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let corner = CORNERS[vertex_index];
    var output: VertexOutput;
    output.clip_position = vec4<f32>(corner, 0.0, 1.0);
    output.uv = corner * 0.5 + vec2<f32>(0.5, 0.5);
    return output;
}

fn hash(point: vec2<f32>) -> f32 {
    return fract(sin(dot(point, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn value_noise(point: vec2<f32>) -> f32 {
    let cell = floor(point);
    let inner = fract(point);
    let blend = inner * inner * (3.0 - 2.0 * inner);
    let bottom = mix(hash(cell), hash(cell + vec2<f32>(1.0, 0.0)), blend.x);
    let top = mix(
        hash(cell + vec2<f32>(0.0, 1.0)),
        hash(cell + vec2<f32>(1.0, 1.0)),
        blend.x,
    );
    return mix(bottom, top, blend.y);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let point = input.uv * info.scale * 6.0 + info.offset;
    let noise = value_noise(point) * 0.6 + value_noise(point * 2.7) * 0.4;
    return vec4<f32>(vec3<f32>(noise), noise * info.strength);
}
";

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    offset: [f32; 2],
    scale: [f32; 2],
    strength: f32,
    padding: [f32; 3],
}

pub struct WindRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl WindRenderer {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wind_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wind_uniform_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wind_uniform_buffer"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wind_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wind_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wind_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group,
        }
    }

    pub fn set_state(&self, queue: &wgpu::Queue, offset: [f32; 2], aspect: f32, strength: f32) {
        let uniform = Uniform {
            offset,
            scale: [aspect, 1.0],
            strength,
            padding: [0.0; 3],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
