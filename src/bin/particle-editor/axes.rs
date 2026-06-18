use wgpu::util::DeviceExt;

const GIZMO_SIZE: f32 = 110.0;
const GIZMO_MARGIN: f32 = 14.0;
const LINE_THICKNESS: f32 = 4.0;

const SHADER: &str = "
struct Uniform {
    view_projection: mat4x4<f32>,
    viewport: vec2<f32>,
    thickness: f32,
};
@group(0) @binding(0) var<uniform> info: Uniform;

struct InstanceInput {
    @location(0) start: vec3<f32>,
    @location(1) end: vec3<f32>,
    @location(2) color: vec3<f32>,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

var<private> CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32, instance: InstanceInput) -> VertexOutput {
    let corner = CORNERS[vertex_index];
    let clip_start = info.view_projection * vec4<f32>(instance.start, 1.0);
    let clip_end = info.view_projection * vec4<f32>(instance.end, 1.0);
    let screen_start = (clip_start.xy / clip_start.w) * info.viewport;
    let screen_end = (clip_end.xy / clip_end.w) * info.viewport;
    let delta = screen_end - screen_start;
    let direction = select(vec2<f32>(1.0, 0.0), normalize(delta), length(delta) > 0.0001);
    let normal = vec2<f32>(-direction.y, direction.x);
    let clip = mix(clip_start, clip_end, corner.x);
    let ndc = clip.xy / clip.w;
    let ndc_offset = normal * corner.y * info.thickness / info.viewport;
    var output: VertexOutput;
    output.clip_position = vec4<f32>((ndc + ndc_offset) * clip.w, clip.z, clip.w);
    output.color = instance.color;
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
";

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct AxisInstance {
    start: [f32; 3],
    end: [f32; 3],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    view_projection: [[f32; 4]; 4],
    viewport: [f32; 2],
    thickness: f32,
    padding: f32,
}

const INSTANCES: [AxisInstance; 3] = [
    AxisInstance {
        start: [0.0, 0.0, 0.0],
        end: [1.0, 0.0, 0.0],
        color: [0.9, 0.2, 0.2],
    },
    AxisInstance {
        start: [0.0, 0.0, 0.0],
        end: [0.0, 1.0, 0.0],
        color: [0.3, 0.85, 0.3],
    },
    AxisInstance {
        start: [0.0, 0.0, 0.0],
        end: [0.0, 0.0, 1.0],
        color: [0.35, 0.5, 0.95],
    },
];

pub struct AxisRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
}

impl AxisRenderer {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("axis_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("axis_uniform_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("axis_uniform_buffer"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("axis_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("axis_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("axis_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<AxisInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 12,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 24,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                    ],
                }],
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
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("axis_instance_buffer"),
            contents: bytemuck::cast_slice(&INSTANCES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group,
            instance_buffer,
        }
    }

    pub fn set_camera(&self, queue: &wgpu::Queue, view_projection: [[f32; 4]; 4]) {
        let uniform = Uniform {
            view_projection,
            viewport: [GIZMO_SIZE, GIZMO_SIZE],
            thickness: LINE_THICKNESS,
            padding: 0.0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn draw(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        surface_width: f32,
        surface_height: f32,
    ) {
        let x = (surface_width - GIZMO_SIZE - GIZMO_MARGIN).max(0.0);
        let y = GIZMO_MARGIN.min((surface_height - GIZMO_SIZE).max(0.0));
        render_pass.set_viewport(x, y, GIZMO_SIZE, GIZMO_SIZE, 0.0, 1.0);
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..6, 0..INSTANCES.len() as u32);
    }
}
