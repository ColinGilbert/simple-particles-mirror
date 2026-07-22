use std::collections::HashMap;

use wgpu::util::DeviceExt;

use crate::effect::{BlendMode, EffectDraw};

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    position: [f32; 3],
    size: f32,
    color: [f32; 4],
    rotation: f32,
    life: f32,
    sheet: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SoftRaw {
    near: f32,
    far: f32,
    fade_distance: f32,
    enabled: f32,
}

const CAMERA_BUFFER_SIZE: u64 = 80;

struct Batch {
    additive: bool,
    texture: String,
    start: u32,
    count: u32,
}

const INSTANCE_LAYOUT: wgpu::VertexBufferLayout = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<InstanceRaw>() as u64,
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
            format: wgpu::VertexFormat::Float32,
        },
        wgpu::VertexAttribute {
            offset: 16,
            shader_location: 2,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 32,
            shader_location: 3,
            format: wgpu::VertexFormat::Float32,
        },
        wgpu::VertexAttribute {
            offset: 36,
            shader_location: 4,
            format: wgpu::VertexFormat::Float32,
        },
        wgpu::VertexAttribute {
            offset: 40,
            shader_location: 5,
            format: wgpu::VertexFormat::Float32x2,
        },
    ],
};

pub struct ParticleRenderer {
    alpha_pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    depth_bind_group_layout: wgpu::BindGroupLayout,
    depth_bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    textures: HashMap<String, wgpu::BindGroup>,
    instance_buffer: wgpu::Buffer,
    instance_capacity: u64,
    instances: Vec<InstanceRaw>,
    batches: Vec<Batch>,
}

impl ParticleRenderer {
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("particle.wgsl").into()),
        });

        let camera_bind_group_layout = make_camera_layout(device);
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle_camera_buffer"),
            size: CAMERA_BUFFER_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle_camera_bind_group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let texture_bind_group_layout = make_texture_layout(device);

        let circle_view = create_circle_texture(device, queue)
            .create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut textures = HashMap::new();
        for name in ["", "circle"] {
            let bind_group =
                make_texture_bind_group(device, &texture_bind_group_layout, &sampler, &circle_view);
            textures.insert(name.to_string(), bind_group);
        }

        let depth_bind_group_layout = make_depth_layout(device);
        let placeholder_depth = make_placeholder_depth(device);
        let depth_bind_group =
            make_depth_bind_group(device, &depth_bind_group_layout, &placeholder_depth);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle_pipeline_layout"),
            bind_group_layouts: &[
                Some(&camera_bind_group_layout),
                Some(&texture_bind_group_layout),
                Some(&depth_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let depth_stencil = depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });

        let alpha_pipeline = make_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            depth_stencil.clone(),
            "particle_alpha_pipeline",
            wgpu::BlendState::ALPHA_BLENDING,
        );
        let additive_pipeline = make_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            depth_stencil,
            "particle_additive_pipeline",
            ADDITIVE_BLEND,
        );

        let instance_capacity = 1024 * std::mem::size_of::<InstanceRaw>() as u64;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle_instance_buffer"),
            size: instance_capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            alpha_pipeline,
            additive_pipeline,
            camera_buffer,
            camera_bind_group,
            texture_bind_group_layout,
            depth_bind_group_layout,
            depth_bind_group,
            sampler,
            textures,
            instance_buffer,
            instance_capacity,
            instances: Vec::new(),
            batches: Vec::new(),
        }
    }

    pub fn set_camera(&self, queue: &wgpu::Queue, view_projection: [[f32; 4]; 4]) {
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&view_projection));
    }

    pub fn set_soft_particles(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene_depth: Option<&wgpu::TextureView>,
        near: f32,
        far: f32,
        fade_distance: f32,
    ) {
        if let Some(view) = scene_depth {
            self.depth_bind_group =
                make_depth_bind_group(device, &self.depth_bind_group_layout, view);
        }
        let soft = SoftRaw {
            near,
            far,
            fade_distance: fade_distance.max(1e-4),
            enabled: if scene_depth.is_some() { 1.0 } else { 0.0 },
        };
        queue.write_buffer(&self.camera_buffer, 64, bytemuck::bytes_of(&soft));
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, draws: &[EffectDraw]) {
        self.instances.clear();
        self.batches.clear();
        for draw in draws {
            let start = self.instances.len() as u32;
            let sheet = [draw.sheet[0].max(1) as f32, draw.sheet[1].max(1) as f32];
            for particle in &draw.instances {
                self.instances.push(InstanceRaw {
                    position: particle.position,
                    size: particle.size,
                    color: particle.color,
                    rotation: particle.rotation,
                    life: particle.life,
                    sheet,
                });
            }
            let count = self.instances.len() as u32 - start;
            if count > 0 {
                self.batches.push(Batch {
                    additive: matches!(draw.blend, BlendMode::Additive),
                    texture: draw.texture.clone(),
                    start,
                    count,
                });
            }
        }

        if self.instances.is_empty() {
            return;
        }
        let data: &[u8] = bytemuck::cast_slice(&self.instances);
        let required = data.len() as u64;
        if required > self.instance_capacity {
            let new_capacity = required.next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particle_instance_buffer"),
                size: new_capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = new_capacity;
        }
        queue.write_buffer(&self.instance_buffer, 0, data);
    }

    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        if self.batches.is_empty() {
            return;
        }
        let Some(fallback) = self.textures.get("") else {
            return;
        };
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_bind_group(2, &self.depth_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        for batch in &self.batches {
            let texture = self.textures.get(&batch.texture).unwrap_or(fallback);
            let pipeline = if batch.additive {
                &self.additive_pipeline
            } else {
                &self.alpha_pipeline
            };
            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(1, texture, &[]);
            render_pass.draw(0..6, batch.start..batch.start + batch.count);
        }
    }

    pub fn register_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        name: &str,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) {
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("particle_texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = make_texture_bind_group(
            device,
            &self.texture_bind_group_layout,
            &self.sampler,
            &view,
        );
        self.textures.insert(name.to_string(), bind_group);
    }
}

const ADDITIVE_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
    depth_stencil: Option<wgpu::DepthStencilState>,
    label: &str,
    blend: wgpu::BlendState,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vertex_main"),
            buffers: &[INSTANCE_LAYOUT],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fragment_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn make_camera_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("particle_camera_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

fn make_texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("particle_texture_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn make_depth_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("particle_depth_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    })
}

fn make_placeholder_depth(device: &wgpu::Device) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("particle_placeholder_depth"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn make_depth_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("particle_depth_bind_group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}

fn make_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("particle_texture_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn create_circle_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let size = 64_u32;
    let center = size as f32 / 2.0;
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let distance = dx.hypot(dy) / center;
            let alpha = (1.0 - distance).clamp(0.0, 1.0);
            let offset = ((y * size + x) * 4) as usize;
            pixels[offset] = 255;
            pixels[offset + 1] = 255;
            pixels[offset + 2] = 255;
            pixels[offset + 3] = (alpha * alpha * 255.0) as u8;
        }
    }
    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("particle_circle_texture"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &pixels,
    )
}
