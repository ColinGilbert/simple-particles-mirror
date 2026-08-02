use std::convert::Infallible;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::Instant;

use document_state::{DiskChange, Document, FileFormat};
use ga3::Vector;
use matrix4::Matrix4;

use simple_particles::effect::{BlendMode, Curve, EmitterShape, Gradient, ParticleEffect};
use simple_particles::render::ParticleRenderer;
use simple_particles::{EmitterId, ParticleSystem};

use super::axes::AxisRenderer;
use super::locale::t;
use super::textures;
use super::ui::build_ui;
use super::wind::WindRenderer;

pub const PREVIEW: &str = "preview";
pub const PREVIEW_FOV: f32 = 45.0;
pub const PREVIEW_DISTANCE: f32 = 5.0;
pub const PREVIEW_TARGET_Y: f32 = 0.5;
pub const PREVIEW_DEFAULT_YAW: f32 = 0.0;
pub const PREVIEW_DEFAULT_PITCH: f32 = 0.35;
const GIZMO_DISTANCE: f32 = 4.0;
const GIZMO_EXTENT: f32 = 1.4;

const BLIT_SHADER: &str = "
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let corner = corners[index];
    var output: VertexOutput;
    output.position = vec4<f32>(corner, 0.0, 1.0);
    output.uv = vec2<f32>(corner.x * 0.5 + 0.5, 0.5 - corner.y * 0.5);
    return output;
}

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(source_texture, source_sampler, input.uv);
}
";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PreviewMotion {
    Static,
    Circle,
    Line,
    Figure8,
    MouseFollow,
}

impl PreviewMotion {
    pub const ALL: [Self; 5] = [
        Self::Static,
        Self::Circle,
        Self::Line,
        Self::Figure8,
        Self::MouseFollow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Static => t("motion_static"),
            Self::Circle => t("motion_circle"),
            Self::Line => t("motion_line"),
            Self::Figure8 => t("motion_figure8"),
            Self::MouseFollow => t("motion_mouse_follow"),
        }
    }
}

#[derive(Clone)]
struct Effect(ParticleEffect);

impl Deref for Effect {
    type Target = ParticleEffect;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Effect {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FileFormat for Effect {
    type Error = Infallible;

    fn load(bytes: &[u8], _path: Option<&Path>) -> Result<Self, Self::Error> {
        let content = std::str::from_utf8(bytes).unwrap_or_default();
        Ok(Self(ParticleEffect::parse(content)))
    }

    fn store(&self) -> Vec<u8> {
        self.0.to_string().into_bytes()
    }
}

pub struct Editor {
    pub motion_mode: PreviewMotion,
    pub path_speed: f32,
    pub follow_delay: f32,
    pub wind: [f32; 3],
    pub background: [f32; 3],
    pub path: String,
    pub status: String,
    pub panel_open: bool,
    pub show_axes: bool,
    pub yaw: f32,
    pub pitch: f32,
    document: Document<Effect>,
    needs_register: bool,
}

impl Editor {
    fn default_editor() -> Self {
        let mut effect = ParticleEffect::parse("");
        effect.texture = "circle".to_string();
        effect.blend = BlendMode::Additive;
        effect.looping = true;
        effect.rate = 80.0;
        effect.shape = EmitterShape::Cone {
            radius: 0.1,
            angle: 28.0,
        };
        effect.lifetime = [0.6, 1.0];
        effect.speed = [2.0, 3.5];
        effect.gravity = [0.0, -2.0, 0.0].into();
        effect.drag = 0.1;
        effect.spin = [-2.0, 2.0];
        effect.size = Curve::linear(0.3, 0.0);
        effect.color = Gradient::linear([1.0, 0.85, 0.4, 1.0], [1.0, 0.3, 0.1, 0.0]);
        Self {
            motion_mode: PreviewMotion::Static,
            path_speed: 1.0,
            follow_delay: 0.15,
            wind: [0.0, 0.0, 0.0],
            background: [0.04, 0.05, 0.08],
            path: "effect.particle".to_string(),
            status: String::new(),
            panel_open: true,
            show_axes: true,
            yaw: PREVIEW_DEFAULT_YAW,
            pitch: PREVIEW_DEFAULT_PITCH,
            document: Document::new(Effect(effect)),
            needs_register: false,
        }
    }

    pub fn effect(&self) -> &ParticleEffect {
        self.document.data()
    }

    pub fn effect_mut(&mut self) -> &mut ParticleEffect {
        self.document.data_mut()
    }

    pub fn save(&mut self) {
        match self.document.save_as(PathBuf::from(&self.path)) {
            Ok(()) => {
                self.status = format!("{}: {}", t("status_saved"), self.path);
            }
            Err(error) => self.status = format!("{}: {error}", t("status_save_failed")),
        }
    }

    pub fn load(&mut self) {
        match Document::open(PathBuf::from(&self.path)) {
            Ok(document) => {
                self.document = document;
                self.needs_register = true;
                self.status = format!("{}: {}", t("status_loaded"), self.path);
            }
            Err(error) => self.status = format!("{}: {error}", t("status_load_failed")),
        }
    }
}

struct Offscreen {
    size: [u32; 2],
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

struct Blit {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl Blit {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle_editor_blit_shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle_editor_blit_layout"),
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
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle_editor_blit_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particle_editor_blit_pipeline"),
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
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("particle_editor_blit_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            sampler,
        }
    }
}

struct BlitCallback {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl egui_wgpu::CallbackTrait for BlitCallback {
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        _resources: &egui_wgpu::CallbackResources,
    ) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

fn pointer_world(pointer: [f32; 2], size: [u32; 2]) -> [f32; 3] {
    let width = size[0].max(1) as f32;
    let height = size[1].max(1) as f32;
    let ndc_x = (pointer[0] / width).mul_add(2.0, -1.0);
    let ndc_y = (pointer[1] / height).mul_add(2.0, -1.0);
    let half_height = PREVIEW_DISTANCE * (PREVIEW_FOV.to_radians() / 2.0).tan();
    let half_width = half_height * (width / height);
    [
        ndc_x * half_width,
        PREVIEW_TARGET_Y - ndc_y * half_height,
        0.0,
    ]
}

/// A particle editor bound to a host's GPU device.
///
/// Holds the edited effect, the live preview system and the renderers. It owns
/// no window and no surface: the preview draws into an offscreen texture that is
/// blitted into whatever [`egui`] pass [`EditorApp::editor_ui`] is called from,
/// so the same app serves the standalone window and an embedded panel.
pub struct EditorApp {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    renderer: ParticleRenderer,
    axes: AxisRenderer,
    wind: WindRenderer,
    wind_offset: [f32; 2],
    system: ParticleSystem,
    editor: Editor,
    preview_id: EmitterId,
    motion_phase: f32,
    follow_pos: [f32; 3],
    last_frame: Instant,
    blit: Blit,
    offscreen: Option<Offscreen>,
}

impl EditorApp {
    #[must_use]
    /// Builds an editor on the caller's device, starting from a default effect.
    ///
    /// `format` is the texture format the host's egui pass renders to; the
    /// offscreen preview target and the blit pipeline are built to match it.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut renderer = ParticleRenderer::new(device, queue, format, None);
        let (grid_pixels, grid_width, grid_height) = textures::grid_sheet();
        renderer.register_texture(device, queue, "grid", &grid_pixels, grid_width, grid_height);
        let axes = AxisRenderer::new(device, format);
        let wind = WindRenderer::new(device, format);
        let blit = Blit::new(device, format);

        let editor = Editor::default_editor();
        let mut system = ParticleSystem::new();
        system.register(PREVIEW, editor.effect().clone());
        let preview_id = system.spawn(PREVIEW, [0.0, 0.0, 0.0]);

        Self {
            device: device.clone(),
            queue: queue.clone(),
            format,
            renderer,
            axes,
            wind,
            wind_offset: [0.0, 0.0],
            system,
            editor,
            preview_id,
            motion_phase: 0.0,
            follow_pos: [0.0, 0.0, 0.0],
            last_frame: Instant::now(),
            blit,
            offscreen: None,
        }
    }

    #[expect(clippy::needless_pass_by_value)]
    /// Loads an effect file into this editor, replacing what it held.
    ///
    /// Mutates the existing app rather than building a new one, so a host can
    /// route repeated clicks to one long-lived tab. A file that cannot be read
    /// or parsed leaves the current effect in place and only sets the status
    /// line.
    pub fn open(&mut self, path: PathBuf) {
        self.editor.path = path.display().to_string();
        match Document::open(&path) {
            Ok(document) => {
                self.editor.document = document;
                self.editor.needs_register = true;
                self.system.register(PREVIEW, self.editor.effect().clone());
            }
            Err(error) => {
                self.editor.status = format!("{}: {error}", t("status_load_failed"));
            }
        }
    }

    /// Draws the controls and the live preview into `ui`, and steps the
    /// simulation by the time since the last call.
    ///
    /// Call once per frame. Prefer [`main_panel`] when embedding, which wraps
    /// this in an [`editor_panels::Panel`].
    pub fn editor_ui(&mut self, ui: &mut egui::Ui) {
        if matches!(self.editor.document.poll(), Some(DiskChange::Reloaded)) {
            self.editor.needs_register = true;
            self.editor.status = format!("{}: {}", t("status_reloaded"), self.editor.path);
        }
        let changed = build_ui(ui, &mut self.editor);
        if changed || self.editor.needs_register {
            self.system.register(PREVIEW, self.editor.effect().clone());
            self.editor.needs_register = false;
        }
        self.viewport_ui(ui);
    }

    /// Writes the effect to `path` and adopts it, returning whether it worked.
    ///
    /// Opens no dialog and never blocks, so a host can call it from a Ctrl+S
    /// handler on its own event loop. Picking the path is the host's job:
    /// [`EditorApp::path`] says whether there already is one.
    pub fn save(&mut self, path: &Path) -> bool {
        match self.editor.document.save_as(path) {
            Ok(()) => {
                self.editor.path = path.display().to_string();
                self.editor.status = format!("{}: {}", t("status_saved"), self.editor.path);
                true
            }
            Err(error) => {
                self.editor.status = format!("{}: {error}", t("status_save_failed"));
                false
            }
        }
    }

    #[must_use]
    /// Where the effect was last opened from or saved to, if anywhere.
    ///
    /// [`None`] for a document that has never been persisted, which is the
    /// host's cue to ask for a path before calling [`EditorApp::save`].
    pub fn path(&self) -> Option<&Path> {
        self.editor.document.path()
    }

    #[must_use]
    /// Whether the effect differs from what was last saved.
    ///
    /// For starring a tab title or guarding a close.
    pub const fn is_modified(&self) -> bool {
        self.editor.document.is_modified()
    }

    fn viewport_ui(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        let delta_time = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        let available = ui.available_size();
        if available.x < 1.0 || available.y < 1.0 {
            return;
        }
        let (rect, response) = ui.allocate_at_least(available, egui::Sense::click_and_drag());
        let pixels_per_point = ui.ctx().pixels_per_point();
        let size = [
            (rect.width() * pixels_per_point).round().max(1.0) as u32,
            (rect.height() * pixels_per_point).round().max(1.0) as u32,
        ];

        if self.editor.motion_mode != PreviewMotion::MouseFollow
            && response.dragged_by(egui::PointerButton::Primary)
        {
            let delta = response.drag_delta();
            self.editor.yaw = delta.x.mul_add(-0.01, self.editor.yaw);
            self.editor.pitch = delta.y.mul_add(0.01, self.editor.pitch).clamp(-1.4, 1.4);
        }
        let pointer_local = response.hover_pos().map(|position| {
            [
                (position.x - rect.min.x) * pixels_per_point,
                (position.y - rect.min.y) * pixels_per_point,
            ]
        });

        let (origin, motion_velocity) = self.step_motion(delta_time, pointer_local, size);

        let wind = self.editor.wind;
        self.system.set_wind(wind);
        self.wind_offset[0] = (wind[0] * delta_time).mul_add(0.5, self.wind_offset[0]);
        self.wind_offset[1] = (wind[1] * delta_time).mul_add(-0.5, self.wind_offset[1]);
        let effective = [
            motion_velocity[0] - wind[0],
            motion_velocity[1] - wind[1],
            motion_velocity[2] - wind[2],
        ];
        self.system
            .set_emitter_motion(self.preview_id, origin, effective);
        self.system.update(delta_time);
        if self.system.is_empty() {
            self.preview_id = self.system.spawn(PREVIEW, origin);
        }

        self.update_cameras(size, wind);
        let draws = self.system.collect();
        self.renderer.prepare(&self.device, &self.queue, &draws);

        self.ensure_offscreen(size);
        let Some(offscreen) = self.offscreen.as_ref() else {
            return;
        };

        let background = self.editor.background.map(f64::from);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("particle_editor_preview_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &offscreen.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: background[0],
                            g: background[1],
                            b: background[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.wind.draw(&mut render_pass);
            self.renderer.draw(&mut render_pass);
            if self.editor.show_axes {
                self.axes
                    .draw(&mut render_pass, size[0] as f32, size[1] as f32);
            }
        }
        self.queue.submit(Some(encoder.finish()));

        let callback = egui_wgpu::Callback::new_paint_callback(
            rect,
            BlitCallback {
                pipeline: self.blit.pipeline.clone(),
                bind_group: offscreen.bind_group.clone(),
            },
        );
        ui.painter().add(callback);
    }

    fn step_motion(
        &mut self,
        delta_time: f32,
        pointer: Option<[f32; 2]>,
        size: [u32; 2],
    ) -> ([f32; 3], [f32; 3]) {
        let radius = 1.5_f32;
        let omega = 2.0_f32 * self.editor.path_speed.max(0.0);
        match self.editor.motion_mode {
            PreviewMotion::Static => ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            PreviewMotion::Circle => {
                self.motion_phase += omega * delta_time;
                let (sin, cos) = self.motion_phase.sin_cos();
                (
                    [cos * radius, 0.0, sin * radius],
                    [-sin * radius * omega, 0.0, cos * radius * omega],
                )
            }
            PreviewMotion::Line => {
                self.motion_phase += omega * delta_time;
                let (sin, cos) = self.motion_phase.sin_cos();
                ([sin * radius, 0.0, 0.0], [cos * radius * omega, 0.0, 0.0])
            }
            PreviewMotion::Figure8 => {
                self.motion_phase += omega * delta_time;
                let (sin, cos) = self.motion_phase.sin_cos();
                (
                    [sin * radius, 0.0, sin * cos * radius],
                    [
                        cos * radius * omega,
                        0.0,
                        cos.mul_add(cos, -(sin * sin)) * radius * omega,
                    ],
                )
            }
            PreviewMotion::MouseFollow => {
                let Some(pointer) = pointer else {
                    return (self.follow_pos, [0.0, 0.0, 0.0]);
                };
                let target = pointer_world(pointer, size);
                let alpha = (delta_time / self.editor.follow_delay.max(0.01)).min(1.0);
                let mut next = self.follow_pos;
                let mut velocity = [0.0; 3];
                for axis in 0..3 {
                    next[axis] = (target[axis] - next[axis]).mul_add(alpha, next[axis]);
                    if delta_time > 1e-5 {
                        velocity[axis] = (next[axis] - self.follow_pos[axis]) / delta_time;
                    }
                }
                self.follow_pos = next;
                (next, velocity)
            }
        }
    }

    fn update_cameras(&self, size: [u32; 2], wind: [f32; 3]) {
        let aspect = size[0] as f32 / size[1].max(1) as f32;
        let center = Vector::new(0.0, PREVIEW_TARGET_Y, 0.0);
        let eye = if self.editor.motion_mode == PreviewMotion::MouseFollow {
            Vector::new(0.0, PREVIEW_TARGET_Y, PREVIEW_DISTANCE)
        } else {
            let yaw = self.editor.yaw;
            let pitch = self.editor.pitch;
            Vector::new(
                PREVIEW_DISTANCE * pitch.cos() * yaw.sin(),
                PREVIEW_DISTANCE.mul_add(pitch.sin(), PREVIEW_TARGET_Y),
                PREVIEW_DISTANCE * pitch.cos() * yaw.cos(),
            )
        };
        let up = Vector::new(0.0, 1.0, 0.0);

        let view_projection = Matrix4::perspective(PREVIEW_FOV.to_radians(), aspect, 0.1, 100.0)
            * Matrix4::look_at(eye, center, up);
        self.renderer
            .set_camera(&self.queue, view_projection.columns());

        let gizmo_eye = (eye - center) * (GIZMO_DISTANCE / PREVIEW_DISTANCE);
        let gizmo_view_projection = Matrix4::orthographic(
            -GIZMO_EXTENT,
            GIZMO_EXTENT,
            -GIZMO_EXTENT,
            GIZMO_EXTENT,
            0.1,
            10.0,
        ) * Matrix4::look_at(gizmo_eye, Vector::new(0.0, 0.0, 0.0), up);
        self.axes
            .set_camera(&self.queue, gizmo_view_projection.columns());
        let wind_speed = wind[2]
            .mul_add(wind[2], wind[0].mul_add(wind[0], wind[1] * wind[1]))
            .sqrt();
        self.wind.set_state(
            &self.queue,
            self.wind_offset,
            aspect,
            (wind_speed * 0.05).min(0.18),
        );
    }

    fn ensure_offscreen(&mut self, size: [u32; 2]) {
        if self
            .offscreen
            .as_ref()
            .is_some_and(|current| current.size == size)
        {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("particle_editor_offscreen"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle_editor_blit_bind_group"),
            layout: &self.blit.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.blit.sampler),
                },
            ],
        });
        self.offscreen = Some(Offscreen {
            size,
            view,
            bind_group,
        });
    }
}

#[must_use]
/// Whether this editor can open `path`, which it can for `.particle` files.
///
/// For a host deciding which tool a clicked file belongs to.
pub fn handles(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "particle")
}

/// The editor as one panel: controls column and 3D preview together.
///
/// Built by [`main_panel`].
pub struct Main<'a> {
    app: &'a mut EditorApp,
}

impl editor_panels::Panel for Main<'_> {
    fn title(&self) -> String {
        "Particles".to_string()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        self.app.editor_ui(ui);
    }
}

#[must_use]
/// Borrows `app` as a panel a host can dock.
pub const fn main_panel(app: &mut EditorApp) -> Main<'_> {
    Main { app }
}
