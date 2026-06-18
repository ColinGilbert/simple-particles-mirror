#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::Instant;

use egui_wgpu::ScreenDescriptor;
use simple_particles::effect::{BlendMode, Curve, EmitterShape, Gradient, ParticleEffect};
use simple_particles::{EmitterId, ParticleSystem, render::ParticleRenderer};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const PREVIEW: &str = "preview";

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { state: None };
    event_loop.run_app(&mut app).expect("run app");
}

struct App {
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            self.state = Some(State::new(event_loop));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if state.overlay.handle_event(&state.window, &event) {
            state.window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => state.render(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.window.request_redraw();
        }
    }
}

struct Editor {
    effect: ParticleEffect,
    size_start: f32,
    size_end: f32,
    color_start: [f32; 4],
    color_end: [f32; 4],
    moving: bool,
    path: String,
    status: String,
}

impl Editor {
    fn default_editor() -> Self {
        let mut editor = Self {
            effect: ParticleEffect::parse(""),
            size_start: 0.3,
            size_end: 0.0,
            color_start: [1.0, 0.85, 0.4, 1.0],
            color_end: [1.0, 0.3, 0.1, 0.0],
            moving: false,
            path: "effect.particle".to_string(),
            status: String::new(),
        };
        editor.effect.texture = "circle".to_string();
        editor.effect.blend = BlendMode::Additive;
        editor.effect.looping = true;
        editor.effect.rate = 80.0;
        editor.effect.shape = EmitterShape::Cone {
            radius: 0.1,
            angle: 28.0,
        };
        editor.effect.lifetime = [0.6, 1.0];
        editor.effect.speed = [2.0, 3.5];
        editor.effect.gravity = [0.0, -2.0, 0.0].into();
        editor.effect.drag = 0.1;
        editor.effect.spin = [-2.0, 2.0];
        editor.rebuild();
        editor
    }

    fn rebuild(&mut self) {
        self.effect.size = Curve::linear(self.size_start, self.size_end);
        self.effect.color = Gradient::linear(self.color_start, self.color_end);
    }

    fn save(&mut self) {
        match std::fs::write(&self.path, self.build_effect().to_string()) {
            Ok(()) => self.status = format!("Gespeichert: {}", self.path),
            Err(error) => self.status = format!("Speichern fehlgeschlagen: {error}"),
        }
    }

    fn load(&mut self) {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => {
                let effect = ParticleEffect::parse(&content);
                self.size_start = effect.size.sample(0.0);
                self.size_end = effect.size.sample(1.0);
                self.color_start = effect.color.sample(0.0);
                self.color_end = effect.color.sample(1.0);
                self.effect = effect;
                self.status = format!("Geladen: {}", self.path);
            }
            Err(error) => self.status = format!("Laden fehlgeschlagen: {error}"),
        }
    }

    fn build_effect(&self) -> ParticleEffect {
        let mut effect = ParticleEffect::parse("");
        effect.texture = self.effect.texture.clone();
        effect.blend = self.effect.blend;
        effect.max_particles = self.effect.max_particles;
        effect.looping = self.effect.looping;
        effect.duration = self.effect.duration;
        effect.rate = self.effect.rate;
        effect.rate_distance = self.effect.rate_distance;
        effect.burst = self.effect.burst;
        effect.shape = self.effect.shape;
        effect.lifetime = self.effect.lifetime;
        effect.speed = self.effect.speed;
        effect.gravity = self.effect.gravity;
        effect.drag = self.effect.drag;
        effect.spin = self.effect.spin;
        effect.size = Curve::linear(self.size_start, self.size_end);
        effect.color = Gradient::linear(self.color_start, self.color_end);
        effect
    }
}

struct State {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: ParticleRenderer,
    system: ParticleSystem,
    overlay: EguiOverlay,
    editor: Editor,
    orbit: f32,
    preview_id: EmitterId,
    motion_phase: f32,
    last_frame: Instant,
}

impl State {
    fn new(event_loop: &ActiveEventLoop) -> Self {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("simple-particles editor"))
                .expect("window"),
        );
        let size = window.inner_size();

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let renderer = ParticleRenderer::new(&device, &queue, format, None);
        let overlay = EguiOverlay::new(&device, format, &window);

        let mut editor = Editor::default_editor();
        if let Some(path) = std::env::args().nth(1) {
            editor.path = path;
            editor.load();
        }
        let mut system = ParticleSystem::new();
        system.register(PREVIEW, editor.build_effect());
        let preview_id = system.spawn(PREVIEW, [0.0, 0.0, 0.0]);

        Self {
            window,
            device,
            queue,
            surface,
            config,
            renderer,
            system,
            overlay,
            editor,
            orbit: 0.0,
            preview_id,
            motion_phase: 0.0,
            last_frame: Instant::now(),
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn render(&mut self) {
        let now = Instant::now();
        let delta_time = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        self.orbit += delta_time * 0.4;

        let radius = 1.5_f32;
        let omega = 2.0_f32;
        let (origin, velocity) = if self.editor.moving {
            self.motion_phase += omega * delta_time;
            let (sin, cos) = self.motion_phase.sin_cos();
            (
                [cos * radius, 0.0, sin * radius],
                [-sin * radius * omega, 0.0, cos * radius * omega],
            )
        } else {
            ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0])
        };
        self.system
            .set_emitter_motion(self.preview_id, origin, velocity);
        self.system.update(delta_time);
        if self.system.is_empty() {
            self.preview_id = self.system.spawn(PREVIEW, origin);
        }

        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let eye = [self.orbit.sin() * 5.0, 2.0, self.orbit.cos() * 5.0];
        let view_projection = multiply(
            perspective(45.0_f32.to_radians(), aspect, 0.1, 100.0),
            look_at(eye, [0.0, 0.5, 0.0], [0.0, 1.0, 0.0]),
        );
        self.renderer.set_camera(&self.queue, view_projection);
        let draws = self.system.collect();
        self.renderer.prepare(&self.device, &self.queue, &draws);

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("particle_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.04,
                            g: 0.05,
                            b: 0.08,
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
            self.renderer.draw(&mut render_pass);
        }
        self.queue.submit(Some(encoder.finish()));

        let mut changed = false;
        let editor = &mut self.editor;
        self.overlay.run(&self.window, |ui| {
            changed = build_ui(ui, editor);
        });
        if changed {
            self.system.register(PREVIEW, self.editor.build_effect());
        }
        let commands = self
            .overlay
            .paint(&self.device, &self.queue, &frame.texture);
        self.queue.submit(commands);

        self.window.pre_present_notify();
        frame.present();
    }
}

fn build_ui(root: &mut egui::Ui, editor: &mut Editor) -> bool {
    let mut changed = false;
    egui::Panel::left("controls")
        .default_size(280.0)
        .show_inside(root, |ui| {
            ui.heading("Partikel-Effekt");
            egui::ScrollArea::vertical().show(ui, |ui| {
                let effect = &mut editor.effect;
                changed |= ui
                    .add(egui::Slider::new(&mut effect.rate, 0.0..=400.0).text("Rate"))
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut effect.rate_distance, 0.0..=200.0)
                            .text("Rate/Distanz"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut effect.lifetime[0], 0.05..=4.0).text("Lifetime min"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut effect.lifetime[1], 0.05..=4.0).text("Lifetime max"),
                    )
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut effect.speed[0], 0.0..=12.0).text("Speed min"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut effect.speed[1], 0.0..=12.0).text("Speed max"))
                    .changed();
                let mut gravity = effect.gravity[1];
                if ui
                    .add(egui::Slider::new(&mut gravity, -20.0..=20.0).text("Gravity Y"))
                    .changed()
                {
                    effect.gravity = [0.0, gravity, 0.0].into();
                    changed = true;
                }
                changed |= ui
                    .add(egui::Slider::new(&mut effect.drag, 0.0..=5.0).text("Drag"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut effect.spin[1], 0.0..=12.0).text("Spin max"))
                    .changed();

                ui.separator();
                changed |= ui
                    .add(egui::Slider::new(&mut editor.size_start, 0.0..=2.0).text("Size start"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut editor.size_end, 0.0..=2.0).text("Size end"))
                    .changed();
                changed |= ui
                    .color_edit_button_rgba_unmultiplied(&mut editor.color_start)
                    .changed();
                changed |= ui
                    .color_edit_button_rgba_unmultiplied(&mut editor.color_end)
                    .changed();

                ui.separator();
                let mut additive = matches!(editor.effect.blend, BlendMode::Additive);
                if ui.checkbox(&mut additive, "Additive Blend").changed() {
                    editor.effect.blend = if additive {
                        BlendMode::Additive
                    } else {
                        BlendMode::Alpha
                    };
                    changed = true;
                }
                ui.checkbox(&mut editor.moving, "Bewegte Vorschau (für Trails)");

                ui.separator();
                ui.label("Emitter-Form");
                let shape = &mut editor.effect.shape;
                let current = shape_name(shape);
                egui::ComboBox::from_id_salt("shape")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for option in SHAPE_OPTIONS {
                            if ui.selectable_label(current == option, option).clicked()
                                && current != option
                            {
                                *shape = default_shape(option);
                                changed = true;
                            }
                        }
                    });
                changed |= shape_params(ui, shape);

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Datei");
                    ui.text_edit_singleline(&mut editor.path);
                });
                ui.horizontal(|ui| {
                    if ui.button("Speichern").clicked() {
                        editor.save();
                    }
                    if ui.button("Laden").clicked() {
                        editor.load();
                        changed = true;
                    }
                });
                if !editor.status.is_empty() {
                    ui.label(&editor.status);
                }
            });
        });
    if changed {
        editor.rebuild();
    }
    changed
}

const SHAPE_OPTIONS: [&str; 9] = [
    "point",
    "sphere",
    "hemisphere",
    "cone",
    "box",
    "disc",
    "ring",
    "line",
    "rect",
];

fn shape_name(shape: &EmitterShape) -> &'static str {
    match shape {
        EmitterShape::Point => "point",
        EmitterShape::Sphere { .. } => "sphere",
        EmitterShape::Hemisphere { .. } => "hemisphere",
        EmitterShape::Cone { .. } => "cone",
        EmitterShape::Box { .. } => "box",
        EmitterShape::Disc { .. } => "disc",
        EmitterShape::Ring { .. } => "ring",
        EmitterShape::Line { .. } => "line",
        EmitterShape::Rect { .. } => "rect",
    }
}

fn default_shape(name: &str) -> EmitterShape {
    match name {
        "sphere" => EmitterShape::Sphere { radius: 0.5 },
        "hemisphere" => EmitterShape::Hemisphere { radius: 0.5 },
        "cone" => EmitterShape::Cone {
            radius: 0.2,
            angle: 25.0,
        },
        "box" => EmitterShape::Box {
            size: [1.0, 1.0, 1.0],
        },
        "disc" => EmitterShape::Disc { radius: 1.0 },
        "ring" => EmitterShape::Ring { radius: 1.0 },
        "line" => EmitterShape::Line { length: 2.0 },
        "rect" => EmitterShape::Rect { size: [2.0, 2.0] },
        _ => EmitterShape::Point,
    }
}

fn shape_params(ui: &mut egui::Ui, shape: &mut EmitterShape) -> bool {
    let mut changed = false;
    match shape {
        EmitterShape::Point => {}
        EmitterShape::Sphere { radius }
        | EmitterShape::Hemisphere { radius }
        | EmitterShape::Disc { radius }
        | EmitterShape::Ring { radius } => {
            changed |= ui
                .add(egui::Slider::new(radius, 0.0..=3.0).text("Radius"))
                .changed();
        }
        EmitterShape::Cone { radius, angle } => {
            changed |= ui
                .add(egui::Slider::new(radius, 0.0..=3.0).text("Radius"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(angle, 0.0..=90.0).text("Winkel"))
                .changed();
        }
        EmitterShape::Box { size } => {
            changed |= ui
                .add(egui::Slider::new(&mut size[0], 0.0..=5.0).text("Breite"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut size[1], 0.0..=5.0).text("Höhe"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut size[2], 0.0..=5.0).text("Tiefe"))
                .changed();
        }
        EmitterShape::Line { length } => {
            changed |= ui
                .add(egui::Slider::new(length, 0.0..=8.0).text("Länge"))
                .changed();
        }
        EmitterShape::Rect { size } => {
            changed |= ui
                .add(egui::Slider::new(&mut size[0], 0.0..=8.0).text("Breite"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut size[1], 0.0..=8.0).text("Tiefe"))
                .changed();
        }
    }
    changed
}

fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let focal = 1.0 / (fovy / 2.0).tan();
    [
        [focal / aspect, 0.0, 0.0, 0.0],
        [0.0, focal, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, near * far / (near - far), 0.0],
    ]
}

fn look_at(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
    let forward = normalize(sub(center, eye));
    let side = normalize(cross(forward, up));
    let upward = cross(side, forward);
    [
        [side[0], upward[0], -forward[0], 0.0],
        [side[1], upward[1], -forward[1], 0.0],
        [side[2], upward[2], -forward[2], 0.0],
        [-dot(side, eye), -dot(upward, eye), dot(forward, eye), 1.0],
    ]
}

fn multiply(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0; 4]; 4];
    for (column_index, column) in b.iter().enumerate() {
        for row in 0..4 {
            result[column_index][row] = (0..4).map(|k| a[k][row] * column[k]).sum();
        }
    }
    result
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    if length > 1e-6 {
        [v[0] / length, v[1] / length, v[2] / length]
    } else {
        v
    }
}

struct EguiOverlay {
    context: egui::Context,
    winit_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    primitives: Vec<egui::ClippedPrimitive>,
    textures_delta: egui::TexturesDelta,
    pixels_per_point: f32,
}

impl EguiOverlay {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, window: &Window) -> Self {
        let context = egui::Context::default();
        let winit_state = egui_winit::State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            window,
            None,
            None,
            None,
        );
        let renderer =
            egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::default());
        Self {
            context,
            winit_state,
            renderer,
            primitives: Vec::new(),
            textures_delta: egui::TexturesDelta::default(),
            pixels_per_point: 1.0,
        }
    }

    fn handle_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.winit_state.on_window_event(window, event).consumed
    }

    fn run(&mut self, window: &Window, build: impl FnMut(&mut egui::Ui)) {
        let mut input = self.winit_state.take_egui_input(window);
        input.focused = window.has_focus();
        let output = self.context.run_ui(input, build);
        self.primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        self.textures_delta = output.textures_delta;
        self.pixels_per_point = output.pixels_per_point;
        self.winit_state
            .handle_platform_output(window, output.platform_output);
    }

    fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame_texture: &wgpu::Texture,
    ) -> Vec<wgpu::CommandBuffer> {
        let view = frame_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let screen = ScreenDescriptor {
            size_in_pixels: [frame_texture.width(), frame_texture.height()],
            pixels_per_point: self.pixels_per_point,
        };
        for (id, image_delta) in &self.textures_delta.set {
            self.renderer
                .update_texture(device, queue, *id, image_delta);
        }
        let user_commands =
            self.renderer
                .update_buffers(device, queue, &mut encoder, &self.primitives, &screen);
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut render_pass = render_pass.forget_lifetime();
            self.renderer
                .render(&mut render_pass, &self.primitives, &screen);
        }
        for id in &self.textures_delta.free {
            self.renderer.free_texture(id);
        }
        user_commands
            .into_iter()
            .chain(std::iter::once(encoder.finish()))
            .collect()
    }
}
