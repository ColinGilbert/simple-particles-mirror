use simple_particles::effect::{BlendMode, Curve, EmitterShape, Gradient, ParticleEffect};

use crate::{
    Editor, PREVIEW_DEFAULT_PITCH, PREVIEW_DEFAULT_YAW, PreviewMotion,
    locale::{self, Language, t},
};

pub fn build_ui(root: &mut egui::Ui, editor: &mut Editor) -> (bool, f32) {
    let mut changed = false;
    if !editor.panel_open {
        egui::Area::new(egui::Id::new("reopen"))
            .fixed_pos([8.0, 8.0])
            .show(root.ctx(), |ui| {
                if ui.button("☰").clicked() {
                    editor.panel_open = true;
                }
            });
        return (false, 0.0);
    }
    let panel = egui::Panel::left("controls")
        .default_size(280.0)
        .show_inside(root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                changed |= file_section(ui, editor);
                changed |= parameter_section(ui, editor);
                preview_section(ui, editor);
            });
        });
    (changed, panel.response.rect.width())
}

fn file_section(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.heading(t("file"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").clicked() {
                editor.panel_open = false;
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label(t("path"));
        ui.text_edit_singleline(&mut editor.path);
    });
    ui.horizontal(|ui| {
        if ui.button(t("save")).clicked() {
            editor.save();
        }
        if ui.button(t("load")).clicked() {
            editor.load();
            changed = true;
        }
    });
    if !editor.status.is_empty() {
        ui.label(&editor.status);
    }
    ui.horizontal(|ui| {
        ui.label(t("language"));
        egui::ComboBox::from_id_salt("language")
            .selected_text(locale::current().native_name())
            .show_ui(ui, |ui| {
                for language in Language::ALL {
                    if ui
                        .selectable_label(locale::current() == language, language.native_name())
                        .clicked()
                    {
                        locale::set(language);
                    }
                }
            });
    });
    changed
}

fn parameter_section(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    let mut changed = false;
    ui.separator();
    ui.heading(t("parameters"));
    ui.label(t("parameters_hint"));
    changed |= emission_fields(ui, &mut editor.effect);
    ui.label(t("size_over_lifetime"));
    changed |= curve_editor(ui, &mut editor.effect.size);
    ui.label(t("color_over_lifetime"));
    changed |= gradient_editor(ui, &mut editor.effect.color);
    let mut additive = matches!(editor.effect.blend, BlendMode::Additive);
    if ui.checkbox(&mut additive, t("additive_blend")).changed() {
        editor.effect.blend = if additive {
            BlendMode::Additive
        } else {
            BlendMode::Alpha
        };
        changed = true;
    }
    ui.label(t("texture"));
    egui::ComboBox::from_id_salt("texture")
        .selected_text(&editor.effect.texture)
        .show_ui(ui, |ui| {
            for option in ["circle", "grid"] {
                if ui
                    .selectable_label(editor.effect.texture == option, option)
                    .clicked()
                    && editor.effect.texture != option
                {
                    editor.effect.texture = option.to_string();
                    changed = true;
                }
            }
        });
    ui.horizontal(|ui| {
        ui.label(t("sheet"));
        changed |= ui
            .add(
                egui::DragValue::new(&mut editor.effect.sheet[0])
                    .range(1..=16)
                    .prefix(format!("{} ", t("sheet_columns"))),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut editor.effect.sheet[1])
                    .range(1..=16)
                    .prefix(format!("{} ", t("sheet_rows"))),
            )
            .changed();
    });
    ui.label(t("emitter_shape"));
    let shape = &mut editor.effect.shape;
    let current = shape_name(shape);
    egui::ComboBox::from_id_salt("shape")
        .selected_text(shape_label(current))
        .show_ui(ui, |ui| {
            for option in SHAPE_OPTIONS {
                if ui
                    .selectable_label(current == option, shape_label(option))
                    .clicked()
                    && current != option
                {
                    *shape = default_shape(option);
                    changed = true;
                }
            }
        });
    changed |= shape_params(ui, shape);

    changed
}

fn preview_section(ui: &mut egui::Ui, editor: &mut Editor) {
    ui.separator();
    ui.heading(t("preview"));
    ui.label(t("preview_hint"));
    egui::ComboBox::from_id_salt("motion")
        .selected_text(editor.motion_mode.label())
        .show_ui(ui, |ui| {
            for mode in PreviewMotion::ALL {
                ui.selectable_value(&mut editor.motion_mode, mode, mode.label());
            }
        });
    if editor.motion_mode == PreviewMotion::MouseFollow {
        ui.add(egui::Slider::new(&mut editor.follow_delay, 0.01..=1.0).text(t("follow_delay")));
    } else if editor.motion_mode != PreviewMotion::Static {
        ui.add(egui::Slider::new(&mut editor.path_speed, 0.0..=4.0).text(t("path_speed")));
    }
    ui.add(egui::Slider::new(&mut editor.wind[0], -8.0..=8.0).text(t("wind_x")));
    ui.add(egui::Slider::new(&mut editor.wind[1], -8.0..=8.0).text(t("wind_y")));
    ui.add(egui::Slider::new(&mut editor.wind[2], -8.0..=8.0).text(t("wind_z")));
    ui.horizontal(|ui| {
        ui.label(t("background"));
        ui.color_edit_button_rgb(&mut editor.background);
    });
    ui.checkbox(&mut editor.show_axes, t("show_axes"));
    if editor.motion_mode != PreviewMotion::MouseFollow {
        ui.label(t("orbit_hint"));
        if ui.button(t("reset_view")).clicked() {
            editor.yaw = PREVIEW_DEFAULT_YAW;
            editor.pitch = PREVIEW_DEFAULT_PITCH;
        }
    }
}

fn curve_editor(ui: &mut egui::Ui, curve: &mut Curve) -> bool {
    let mut keys = curve.keys().to_vec();
    let count = keys.len();
    let mut changed = false;
    let mut remove = None;
    for (index, key) in keys.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut key[0])
                        .speed(0.01)
                        .range(0.0..=1.0)
                        .prefix(format!("{} ", t("time"))),
                )
                .changed();
            changed |= ui
                .add(
                    egui::DragValue::new(&mut key[1])
                        .speed(0.01)
                        .range(0.0..=8.0)
                        .prefix(format!("{} ", t("size"))),
                )
                .changed();
            if count > 1 && ui.button("−").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        keys.remove(index);
        changed = true;
    } else if ui.button(t("add_key")).clicked() {
        let last = keys.last().copied().unwrap_or([0.0, 1.0]);
        keys.push([(last[0] + 0.25).min(1.0), last[1]]);
        changed = true;
    }
    if changed {
        *curve = Curve::from_keys(keys);
    }
    changed
}

fn gradient_editor(ui: &mut egui::Ui, gradient: &mut Gradient) -> bool {
    let mut stops = gradient.stops().to_vec();
    let count = stops.len();
    let mut changed = false;
    let mut remove = None;
    for (index, stop) in stops.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut stop.0)
                        .speed(0.01)
                        .range(0.0..=1.0)
                        .prefix(format!("{} ", t("time"))),
                )
                .changed();
            changed |= ui
                .color_edit_button_rgba_unmultiplied(&mut stop.1)
                .changed();
            if count > 1 && ui.button("−").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        stops.remove(index);
        changed = true;
    } else if ui.button(t("add_stop")).clicked() {
        let last = stops.last().copied().unwrap_or((0.0, [1.0; 4]));
        stops.push(((last.0 + 0.25).min(1.0), last.1));
        changed = true;
    }
    if changed {
        *gradient = Gradient::from_stops(stops);
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

const fn shape_name(shape: &EmitterShape) -> &'static str {
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

fn shape_label(name: &str) -> &'static str {
    match name {
        "sphere" => t("shape_sphere"),
        "hemisphere" => t("shape_hemisphere"),
        "cone" => t("shape_cone"),
        "box" => t("shape_box"),
        "disc" => t("shape_disc"),
        "ring" => t("shape_ring"),
        "line" => t("shape_line"),
        "rect" => t("shape_rect"),
        _ => t("shape_point"),
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
                .add(egui::Slider::new(radius, 0.0..=3.0).text(t("radius")))
                .changed();
        }
        EmitterShape::Cone { radius, angle } => {
            changed |= ui
                .add(egui::Slider::new(radius, 0.0..=3.0).text(t("radius")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(angle, 0.0..=90.0).text(t("angle")))
                .changed();
        }
        EmitterShape::Box { size } => {
            changed |= ui
                .add(egui::Slider::new(&mut size[0], 0.0..=5.0).text(t("width")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut size[1], 0.0..=5.0).text(t("height")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut size[2], 0.0..=5.0).text(t("depth")))
                .changed();
        }
        EmitterShape::Line { length } => {
            changed |= ui
                .add(egui::Slider::new(length, 0.0..=8.0).text(t("length")))
                .changed();
        }
        EmitterShape::Rect { size } => {
            changed |= ui
                .add(egui::Slider::new(&mut size[0], 0.0..=8.0).text(t("width")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut size[1], 0.0..=8.0).text(t("depth")))
                .changed();
        }
    }
    changed
}

fn emission_fields(ui: &mut egui::Ui, effect: &mut ParticleEffect) -> bool {
    let mut changed = false;
    changed |= ui
        .add(egui::Slider::new(&mut effect.rate, 0.0..=400.0).text(t("rate")))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut effect.rate_distance, 0.0..=200.0).text(t("rate_distance")))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut effect.lifetime[0], 0.05..=4.0).text(t("lifetime_min")))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut effect.lifetime[1], 0.05..=4.0).text(t("lifetime_max")))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut effect.speed[0], 0.0..=12.0).text(t("speed_min")))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut effect.speed[1], 0.0..=12.0).text(t("speed_max")))
        .changed();
    let mut gravity: [f32; 3] = effect.gravity.into();
    let mut gravity_changed = false;
    for (axis, key) in ["gravity_x", "gravity_y", "gravity_z"].iter().enumerate() {
        gravity_changed |= ui
            .add(egui::Slider::new(&mut gravity[axis], -20.0..=20.0).text(t(key)))
            .changed();
    }
    if gravity_changed {
        effect.gravity = gravity.into();
        changed = true;
    }
    changed |= ui
        .add(egui::Slider::new(&mut effect.drag, 0.0..=5.0).text(t("drag")))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut effect.spin[0], -12.0..=12.0).text(t("spin_min")))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut effect.spin[1], -12.0..=12.0).text(t("spin_max")))
        .changed();
    changed |= ui
        .add(
            egui::DragValue::new(&mut effect.max_particles)
                .range(1..=100_000)
                .prefix(format!("{} ", t("max_particles"))),
        )
        .changed();
    changed |= ui
        .add(
            egui::DragValue::new(&mut effect.burst)
                .range(0..=1_000)
                .prefix(format!("{} ", t("burst"))),
        )
        .changed();
    changed |= ui.checkbox(&mut effect.looping, t("looping")).changed();
    if !effect.looping {
        changed |= ui
            .add(egui::Slider::new(&mut effect.duration, 0.05..=10.0).text(t("duration")))
            .changed();
    }
    changed
}
