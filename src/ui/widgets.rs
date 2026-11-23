//! Small reusable pieces of UI.

use super::theme::{kelvin_color, pal, regular, semibold, Icon};
use crate::model::PxRect;
use eframe::egui::{
    self, epaint::RectShape, pos2, text::LayoutJob, text::TextWrapping, vec2, Align, Align2,
    Color32, CornerRadius, FontId, Frame, Layout, Margin, Mesh, Rect, Response, RichText, Sense,
    Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
};

/// Space between cards side by side, and the padding inside a card.
pub const GAP: f32 = 12.0;
const PAD: f32 = 16.0;

/// A rounded card.
pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(PAD as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
}

/// Cards side by side with the same height; `weights` sets their widths.
pub fn card_row(ui: &mut Ui, weights: &[f32], add: &mut dyn FnMut(&mut Ui, usize)) {
    let p = pal(ui);
    let n = weights.len().max(1);
    let total = ui.available_width();
    let usable = total - GAP * (n as f32 - 1.0);
    let sum: f32 = weights.iter().sum::<f32>().max(0.001);
    let origin = ui.cursor().min;
    let mut x = origin.x;
    let mut bottom = origin.y;
    let mut frames = Vec::with_capacity(n);
    for (i, weight) in weights.iter().enumerate() {
        let width = (usable * weight / sum).floor();
        let background = ui.painter().add(Shape::Noop);
        let inner = Rect::from_min_max(
            pos2(x + PAD, origin.y + PAD),
            pos2(x + width - PAD, origin.y + PAD + 6000.0),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt(("card-row", i))
                .max_rect(inner)
                .layout(Layout::top_down(Align::Min)),
        );
        child.set_width(inner.width());
        add(&mut child, i);
        bottom = bottom.max(child.min_rect().bottom() + PAD);
        frames.push((background, x, width));
        x += width + GAP;
    }
    for (background, x, width) in frames {
        let rect = Rect::from_min_max(pos2(x, origin.y), pos2(x + width, bottom));
        ui.painter().set(
            background,
            RectShape::new(
                rect,
                CornerRadius::same(12),
                p.card,
                Stroke::new(1.0_f32, p.border),
                StrokeKind::Inside,
            ),
        );
    }
    ui.allocate_rect(
        Rect::from_min_max(origin, pos2(origin.x + total, bottom)),
        Sense::hover(),
    );
}

fn filled_button(ui: &mut Ui, text: &str, fill: Color32, fg: Color32, enabled: bool) -> Response {
    let p = pal(ui);
    let (fill, fg) = if enabled {
        (fill, fg)
    } else {
        (p.hover, p.faint)
    };
    let button = egui::Button::new(RichText::new(text).font(semibold(14.0)).color(fg))
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(8))
        .min_size(vec2(0.0, 34.0));
    let prev = ui.spacing().button_padding;
    ui.spacing_mut().button_padding = vec2(16.0, 7.0);
    let r = ui.add_enabled(enabled, button);
    ui.spacing_mut().button_padding = prev;
    if r.hovered() && enabled {
        ui.painter()
            .rect_filled(r.rect, CornerRadius::same(8), Color32::from_white_alpha(22));
    }
    r
}

/// The main action on a screen.
pub fn primary_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    let p = pal(ui);
    filled_button(ui, text, p.accent, p.on_accent, enabled)
}

/// A secondary action.
pub fn ghost_button(ui: &mut Ui, text: &str) -> Response {
    ghost_button_enabled(ui, text, true)
}

pub fn ghost_button_enabled(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    let p = pal(ui);
    let button =
        egui::Button::new(RichText::new(text).color(if enabled { p.text } else { p.faint }))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(8))
            .min_size(vec2(0.0, 34.0));
    ui.add_enabled(enabled, button)
}

/// A small secondary action (inside rows).
pub fn small_button(ui: &mut Ui, text: &str) -> Response {
    let p = pal(ui);
    let button = egui::Button::new(RichText::new(text).size(13.0).color(p.text))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(7))
        .min_size(vec2(0.0, 28.0));
    ui.add(button)
}

/// A square icon-only button with a tooltip.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    icon_button_enabled(ui, icon, tooltip, true)
}

pub fn icon_button_enabled(ui: &mut Ui, icon: Icon, tooltip: &str, enabled: bool) -> Response {
    let p = pal(ui);
    let label = if icon.glyph().is_empty() {
        tooltip.chars().take(1).collect::<String>()
    } else {
        icon.glyph().to_string()
    };
    let button = egui::Button::new(RichText::new(label).size(15.0).color(if enabled {
        p.muted
    } else {
        p.border
    }))
    .fill(Color32::TRANSPARENT)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(8))
    .min_size(vec2(32.0, 32.0));
    ui.add_enabled(enabled, button).on_hover_text(tooltip)
}

/// A small pill label.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = semibold(11.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let size = vec2(galley.size().x + 14.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), color.gamma_multiply(0.16));
    let pos = pos2(
        rect.center().x - galley.size().x / 2.0,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(pos, galley, color);
    resp
}

/// Paints a custom checkbox in `rect`.
pub fn paint_checkbox(ui: &Ui, rect: Rect, checked: bool, hovered: bool) {
    let p = pal(ui);
    let r = Rect::from_center_size(rect.center(), Vec2::splat(18.0));
    if checked {
        ui.painter().rect_filled(r, CornerRadius::same(5), p.accent);
        let c = r.center();
        let pts = [
            c + vec2(-4.5, 0.0),
            c + vec2(-1.5, 3.2),
            c + vec2(4.8, -3.6),
        ];
        ui.painter()
            .line_segment([pts[0], pts[1]], Stroke::new(2.0_f32, p.on_accent));
        ui.painter()
            .line_segment([pts[1], pts[2]], Stroke::new(2.0_f32, p.on_accent));
    } else {
        ui.painter().rect_stroke(
            r,
            CornerRadius::same(5),
            Stroke::new(1.5_f32, if hovered { p.accent } else { p.faint }),
            StrokeKind::Inside,
        );
    }
}

/// A checkbox with a label. `changed()` is set when it is clicked.
pub fn checkbox(ui: &mut Ui, value: &mut bool, label: &str) -> Response {
    let p = pal(ui);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), regular(14.0), p.text);
    let size = vec2(
        26.0 + galley.size().x - if label.is_empty() { 6.0 } else { 0.0 },
        24.0,
    );
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *value = !*value;
        resp.mark_changed();
    }
    let box_rect = Rect::from_min_size(rect.min, vec2(20.0, rect.height()));
    paint_checkbox(ui, box_rect, *value, resp.hovered());
    ui.painter().galley(
        pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0),
        galley,
        p.text,
    );
    resp
}

/// An on/off switch. Returns the response; the caller flips the value on click.
pub fn toggle(ui: &mut Ui, on: bool, enabled: bool) -> Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(38.0, 22.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let t = ui.ctx().animate_bool_with_time(resp.id, on, 0.12);
    let track = if !enabled {
        p.border
    } else if on {
        p.accent
    } else {
        p.faint.gamma_multiply(0.6)
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(11), track);
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    let knob = if !enabled {
        p.faint
    } else if on {
        p.on_accent
    } else {
        Color32::WHITE
    };
    ui.painter()
        .circle_filled(pos2(x, rect.center().y), 8.0, knob);
    resp
}

/// A row of mutually exclusive choices. Returns the newly picked index.
pub fn segmented(ui: &mut Ui, options: &[&str], current: usize) -> Option<usize> {
    let p = pal(ui);
    let mut picked = None;
    Frame::new()
        .fill(p.hover)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                // In a right-to-left row the first widget lands on the right:
                // add the options backwards so they still read left to right.
                let mut order: Vec<usize> = (0..options.len()).collect();
                if ui.layout().prefer_right_to_left() {
                    order.reverse();
                }
                for i in order {
                    let label = &options[i];
                    let active = i == current;
                    let text = RichText::new(*label).color(if active { p.text } else { p.muted });
                    let text = if active {
                        text.font(semibold(13.0))
                    } else {
                        text.size(13.0)
                    };
                    let b = egui::Button::new(text)
                        .fill(if active { p.card } else { Color32::TRANSPARENT })
                        .stroke(if active {
                            Stroke::new(1.0_f32, p.border)
                        } else {
                            Stroke::NONE
                        })
                        .corner_radius(CornerRadius::same(7))
                        .min_size(vec2(0.0, 26.0));
                    if ui.add(b).clicked() && !active {
                        picked = Some(i);
                    }
                }
            });
        });
    picked
}

/// A chip (presets, filters).
pub fn chip(ui: &mut Ui, label: &str, active: bool) -> Response {
    let p = pal(ui);
    let (fill, fg, stroke) = if active {
        (
            p.accent.gamma_multiply(0.16),
            p.accent,
            Stroke::new(1.0_f32, p.accent.gamma_multiply(0.6)),
        )
    } else {
        (Color32::TRANSPARENT, p.text, Stroke::new(1.0_f32, p.border))
    };
    let text = RichText::new(label).size(13.5).color(fg);
    let text = if active {
        text.font(semibold(13.5))
    } else {
        text
    };
    let b = egui::Button::new(text)
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(16))
        .min_size(vec2(0.0, 32.0));
    ui.add(b)
}

/// Lays out one line of text, cut off with an ellipsis at `max_width`.
pub fn elided(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = TextWrapping {
        max_width: max_width.max(8.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts(|f| f.layout_job(job))
}

/// Paints one elided line, vertically centred in `rect`.
pub fn text_in(ui: &Ui, rect: Rect, text: &str, font: FontId, color: Color32) {
    let galley = elided(ui, text, font, color, rect.width());
    ui.painter().galley(
        pos2(rect.left(), rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
}

/// A designed empty / error state: icon, title, explanation.
pub fn empty_state(ui: &mut Ui, icon: Icon, color: Color32, title: &str, text: &str) {
    let p = pal(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(18.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 32.0, color.gamma_multiply(0.14));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon.glyph(),
            regular(26.0),
            color,
        );
        ui.add_space(8.0);
        ui.label(RichText::new(title).font(semibold(17.0)).color(p.text));
        ui.scope(|ui| {
            ui.set_max_width(460.0);
            ui.label(RichText::new(text).color(p.muted));
        });
        ui.add_space(12.0);
    });
}

/// Section title inside a card.
pub fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).font(semibold(15.0)));
}

/// Section title with an icon in the accent colour.
pub fn section_icon(ui: &mut Ui, icon: Icon, title: &str) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if !icon.glyph().is_empty() {
            ui.label(RichText::new(icon.glyph()).size(15.0).color(p.accent));
        }
        ui.label(RichText::new(title).font(semibold(15.0)));
    });
}

/// Muted explanatory text that wraps.
pub fn hint(ui: &mut Ui, text: &str) {
    let p = pal(ui);
    ui.add(egui::Label::new(RichText::new(text).size(12.5).color(p.muted)).wrap());
}

/// A thin horizontal line.
pub fn divider(ui: &mut Ui) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, p.border);
}

/// The frame used by modal dialogs.
pub fn modal_frame(ctx: &egui::Context) -> Frame {
    let p = super::theme::pal_ctx(ctx);
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(22))
}

/// A right-aligned row of buttons with a fixed height. A bare right-to-left
/// layout takes all the height its parent may use, which stretches modals and
/// cards to the full window; this does not.
pub fn button_row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 36.0),
        Layout::right_to_left(Align::Center),
        add,
    );
}

/// A fixed-height row: `left` laid out left to right, `right` right to left.
pub fn split_row(
    ui: &mut Ui,
    height: f32,
    left: impl FnOnce(&mut Ui),
    right: impl FnOnce(&mut Ui),
) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let mut l = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    left(&mut l);
    let used = l.min_rect().right();
    let rrect = Rect::from_min_max(pos2(used.min(rect.right()), rect.top()), rect.max);
    let mut r = ui.new_child(
        UiBuilder::new()
            .max_rect(rrect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    right(&mut r);
}

/// One setting: title and explanation on the left, its control on the right.
/// The row has a fixed height, so the right-aligned control cannot stretch
/// the card it sits in.
pub fn setting_row(ui: &mut Ui, title: &str, hint: &str, control: impl FnOnce(&mut Ui)) {
    setting_row_w(ui, title, hint, 230.0, control)
}

/// [`setting_row`] with a chosen width for the control.
pub fn setting_row_w(
    ui: &mut Ui,
    title: &str,
    hint: &str,
    control_width: f32,
    control: impl FnOnce(&mut Ui),
) {
    let p = pal(ui);
    let width = ui.available_width();
    let text_width = (width - control_width).max(120.0);
    let hint_galley = (!hint.is_empty()).then(|| {
        ui.painter()
            .layout(hint.to_string(), regular(12.0), p.muted, text_width - 8.0)
    });
    let height = match &hint_galley {
        Some(g) => (26.0 + g.size().y + 6.0).max(44.0),
        None => 38.0,
    };
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let title_pos = match &hint_galley {
        Some(_) => pos2(rect.left(), rect.top() + 4.0),
        None => pos2(rect.left(), rect.center().y - 10.0),
    };
    ui.painter()
        .text(title_pos, Align2::LEFT_TOP, title, regular(14.0), p.text);
    if let Some(g) = hint_galley {
        ui.painter()
            .galley(pos2(rect.left(), rect.top() + 25.0), g, p.muted);
    }
    let control_rect =
        Rect::from_min_max(pos2(rect.left() + text_width + 8.0, rect.top()), rect.max);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(control_rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    control(&mut child);
}

/// A label and value pair, stacked (for detail grids).
pub fn field(ui: &mut Ui, label: &str, value: &str) {
    let p = pal(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(RichText::new(label).size(12.0).color(p.muted));
        ui.add(egui::Label::new(RichText::new(value).size(14.0)).wrap());
    });
}

/// A grid of [`field`]s in `cols` columns.
pub fn field_grid(ui: &mut Ui, id: &str, cols: usize, items: &[(&str, String)]) {
    let cols = cols.max(1);
    let col_w = (ui.available_width() - 12.0 * (cols as f32 - 1.0)) / cols as f32;
    egui::Grid::new(id)
        .num_columns(cols)
        .spacing(vec2(12.0, 8.0))
        .min_col_width(col_w)
        .max_col_width(col_w)
        .show(ui, |ui| {
            for (i, (label, value)) in items.iter().enumerate() {
                field(ui, label, value);
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
}

// ----------------------------------------------------------------------------------
// Sliders
// ----------------------------------------------------------------------------------

/// How a [`level_slider`] paints its track.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Track {
    /// Accent fill up to the value.
    Plain,
    /// Like `Plain`, with the part below zero marked as software dimming.
    Brightness,
    /// Warm to neutral colour temperature.
    Kelvin,
    /// A single colour channel (RGB gains).
    Channel(Color32),
}

/// Where a value sits on a slider, 0..=1.
pub fn slider_fraction(value: i32, min: i32, max: i32) -> f32 {
    if max <= min {
        return 0.0;
    }
    ((value.clamp(min, max) - min) as f32 / (max - min) as f32).clamp(0.0, 1.0)
}

/// The value at a place on a slider, rounded to `step`.
pub fn slider_value(fraction: f32, min: i32, max: i32, step: i32) -> i32 {
    let raw = min as f32 + fraction.clamp(0.0, 1.0) * (max - min) as f32;
    let step = step.max(1) as f32;
    let v = ((raw - min as f32) / step).round() * step + min as f32;
    (v.round() as i32).clamp(min, max)
}

/// A slider of whole numbers with a custom track. It reports `changed()`
/// while dragging, so values can be sent as they move. Arrow keys move it
/// by `step` when it has focus.
pub fn level_slider(
    ui: &mut Ui,
    value: &mut i32,
    range: std::ops::RangeInclusive<i32>,
    step: i32,
    width: f32,
    track: Track,
    enabled: bool,
) -> Response {
    let p = pal(ui);
    let (min, max) = (*range.start(), *range.end());
    let height = 26.0;
    let (rect, mut resp) = ui.allocate_exact_size(
        vec2(width.max(40.0), height),
        if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let knob_r = 8.0;
    let x0 = rect.left() + knob_r;
    let x1 = rect.right() - knob_r;
    let before = *value;
    if enabled {
        if let Some(pos) = resp.interact_pointer_pos() {
            if resp.is_pointer_button_down_on() || resp.clicked() || resp.dragged() {
                let f = ((pos.x - x0) / (x1 - x0)).clamp(0.0, 1.0);
                *value = slider_value(f, min, max, step);
                resp.request_focus();
            }
        }
        if resp.has_focus() {
            let (left, right, pgdn, pgup) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::PageDown),
                    i.key_pressed(egui::Key::PageUp),
                )
            });
            let mut delta = 0;
            if left {
                delta -= step;
            }
            if right {
                delta += step;
            }
            if pgdn {
                delta -= step * 10;
            }
            if pgup {
                delta += step * 10;
            }
            if delta != 0 {
                *value = (*value + delta).clamp(min, max);
            }
        }
    }
    if *value != before {
        resp.mark_changed();
    }

    let cy = rect.center().y;
    let track_rect = Rect::from_min_max(pos2(x0 - 3.0, cy - 3.0), pos2(x1 + 3.0, cy + 3.0));
    let frac = slider_fraction(*value, min, max);
    let kx = egui::lerp(x0..=x1, frac);
    let painter = ui.painter();
    let accent = if enabled { p.accent } else { p.faint };
    match track {
        Track::Kelvin => {
            // A gradient from warm to neutral.
            let mut mesh = Mesh::default();
            let n = 24;
            for i in 0..=n {
                let f = i as f32 / n as f32;
                let k = min as f32 + f * (max - min) as f32;
                let mut c = kelvin_color(k as u32);
                if !enabled {
                    c = c.gamma_multiply(0.4);
                }
                let x = egui::lerp(track_rect.left()..=track_rect.right(), f);
                mesh.colored_vertex(pos2(x, track_rect.top()), c);
                mesh.colored_vertex(pos2(x, track_rect.bottom()), c);
                if i > 0 {
                    let b = (i as u32) * 2;
                    mesh.add_triangle(b - 2, b - 1, b);
                    mesh.add_triangle(b - 1, b, b + 1);
                }
            }
            painter.add(Shape::mesh(mesh));
            painter.rect_stroke(
                track_rect,
                CornerRadius::same(3),
                Stroke::new(1.0_f32, p.border),
                StrokeKind::Outside,
            );
        }
        _ => {
            painter.rect_filled(track_rect, CornerRadius::same(3), p.border);
            let fill_color = match track {
                Track::Channel(c) if enabled => c,
                _ => accent,
            };
            if track == Track::Brightness && min < 0 {
                // The software-dimming zone below zero.
                let zx = egui::lerp(x0..=x1, slider_fraction(0, min, max));
                let zone = Rect::from_min_max(track_rect.min, pos2(zx, track_rect.bottom()));
                painter.rect_filled(zone, CornerRadius::same(3), p.info.gamma_multiply(0.35));
                painter.line_segment(
                    [pos2(zx, cy - 7.0), pos2(zx, cy + 7.0)],
                    Stroke::new(1.0_f32, p.faint),
                );
                if *value < 0 {
                    let fill = Rect::from_min_max(
                        pos2(kx.min(zx), track_rect.top()),
                        pos2(zx, track_rect.bottom()),
                    );
                    painter.rect_filled(fill, CornerRadius::same(3), p.info);
                } else {
                    let fill = Rect::from_min_max(
                        pos2(zx, track_rect.top()),
                        pos2(kx, track_rect.bottom()),
                    );
                    painter.rect_filled(fill, CornerRadius::same(3), fill_color);
                }
            } else {
                let fill = Rect::from_min_max(track_rect.min, pos2(kx, track_rect.bottom()));
                painter.rect_filled(fill, CornerRadius::same(3), fill_color);
            }
        }
    }
    let hovered = resp.hovered() || resp.dragged() || resp.has_focus();
    let knob_fill = if enabled { Color32::WHITE } else { p.faint };
    let ring = if !enabled {
        p.border
    } else if track == Track::Brightness && *value < 0 {
        p.info
    } else {
        accent
    };
    painter.circle(
        pos2(kx, cy),
        if hovered { knob_r + 1.0 } else { knob_r },
        knob_fill,
        Stroke::new(if hovered { 3.0_f32 } else { 2.5 }, ring),
    );
    resp
}

/// "55 %", or "-20 %" for software dimming.
pub fn percent_text(value: i32) -> String {
    format!("{value} %")
}

/// A slider with a value label at its right, filling the available width.
pub fn slider_with_value(
    ui: &mut Ui,
    value: &mut i32,
    range: std::ops::RangeInclusive<i32>,
    step: i32,
    track: Track,
    enabled: bool,
    label: impl Fn(i32) -> String,
) -> Response {
    let p = pal(ui);
    let label_w = 64.0;
    ui.horizontal(|ui| {
        let w = (ui.available_width() - label_w - 8.0).max(60.0);
        let resp = level_slider(ui, value, range, step, w, track, enabled);
        let (rect, _) = ui.allocate_exact_size(vec2(label_w, 26.0), Sense::hover());
        let color = if !enabled {
            p.faint
        } else if track == Track::Brightness && *value < 0 {
            p.info
        } else {
            p.text
        };
        ui.painter().text(
            rect.right_center(),
            Align2::RIGHT_CENTER,
            label(*value),
            semibold(14.0),
            color,
        );
        resp
    })
    .inner
}

// ----------------------------------------------------------------------------------
// The monitor layout picture
// ----------------------------------------------------------------------------------

/// The rectangle of each desktop rectangle inside `area`, keeping the
/// arrangement and proportions, centred.
pub fn layout_rects(rects: &[PxRect], area: Rect) -> Vec<Rect> {
    if rects.is_empty() {
        return Vec::new();
    }
    let left = rects.iter().map(|r| r.x).min().unwrap_or(0) as f32;
    let top = rects.iter().map(|r| r.y).min().unwrap_or(0) as f32;
    let right = rects.iter().map(|r| r.right()).max().unwrap_or(1) as f32;
    let bottom = rects.iter().map(|r| r.bottom()).max().unwrap_or(1) as f32;
    let scale =
        (area.width() / (right - left).max(1.0)).min(area.height() / (bottom - top).max(1.0));
    let used = vec2((right - left) * scale, (bottom - top) * scale);
    let origin = area.center() - used / 2.0;
    rects
        .iter()
        .map(|r| {
            Rect::from_min_size(
                origin + vec2((r.x as f32 - left) * scale, (r.y as f32 - top) * scale),
                vec2(r.w.max(1) as f32 * scale, r.h.max(1) as f32 * scale),
            )
        })
        .collect()
}

/// One screen in the layout picture.
pub struct LayoutItem {
    pub rect: PxRect,
    pub number: usize,
    pub name: String,
    pub primary: bool,
    pub selected: bool,
}

/// Paints the arrangement of the screens. Returns the index clicked, if any.
pub fn layout_picture(ui: &mut Ui, items: &[LayoutItem], height: f32) -> Option<usize> {
    let p = pal(ui);
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let rects: Vec<PxRect> = items.iter().map(|i| i.rect).collect();
    let mut clicked = None;
    for (i, (item, r)) in items
        .iter()
        .zip(layout_rects(&rects, area.shrink2(vec2(24.0, 4.0))))
        .enumerate()
    {
        let r = r.shrink(3.0);
        let resp = ui.interact(r, ui.id().with(("screen", i)), Sense::click());
        if resp.clicked() {
            clicked = Some(i);
        }
        let (fill, stroke) = if item.selected {
            (p.selected, p.accent)
        } else if resp.hovered() {
            (p.hover, p.muted)
        } else {
            (p.hover, p.faint)
        };
        ui.painter().rect(
            r,
            CornerRadius::same(6),
            fill,
            Stroke::new(if item.selected { 2.0_f32 } else { 1.5 }, stroke),
            StrokeKind::Inside,
        );
        let big = (r.height() * 0.3).clamp(14.0, 26.0);
        ui.painter().text(
            r.center() - vec2(0.0, if r.height() > 60.0 { 9.0 } else { 0.0 }),
            Align2::CENTER_CENTER,
            item.number.to_string(),
            semibold(big),
            if item.selected { p.accent } else { p.text },
        );
        if r.height() > 60.0 {
            let name_rect =
                Rect::from_center_size(r.center() + vec2(0.0, 14.0), vec2(r.width() - 10.0, 16.0));
            let g = elided(ui, &item.name, regular(11.5), p.muted, name_rect.width());
            ui.painter().galley(
                pos2(name_rect.center().x - g.size().x / 2.0, name_rect.top()),
                g,
                p.muted,
            );
        }
        if item.primary {
            let star = Rect::from_min_size(r.left_top() + vec2(6.0, 6.0), vec2(16.0, 16.0));
            ui.painter().circle_filled(star.center(), 7.0, p.accent);
            ui.painter().text(
                star.center(),
                Align2::CENTER_CENTER,
                "P",
                semibold(9.5),
                p.on_accent,
            );
        }
        resp.on_hover_text(if item.primary {
            format!("{}. {} (main display)", item.number, item.name)
        } else {
            format!("{}. {}", item.number, item.name)
        });
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_mapping_round_trips() {
        assert_eq!(slider_fraction(0, 0, 100), 0.0);
        assert_eq!(slider_fraction(100, 0, 100), 1.0);
        assert_eq!(slider_fraction(-50, -50, 100), 0.0);
        assert!((slider_fraction(0, -50, 100) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(slider_fraction(500, 0, 100), 1.0);
        assert_eq!(slider_fraction(5, 5, 5), 0.0);
        for v in [-50, -20, 0, 1, 37, 99, 100] {
            assert_eq!(slider_value(slider_fraction(v, -50, 100), -50, 100, 1), v);
        }
        // Kelvin in steps of 50 from 1900.
        assert_eq!(slider_value(0.5, 1900, 6500, 50), 4200);
        assert_eq!(slider_value(1.2, 1900, 6500, 50), 6500);
        assert_eq!(slider_value(-1.0, 1900, 6500, 50), 1900);
    }

    #[test]
    fn layout_keeps_proportions_and_fits() {
        let area = Rect::from_min_size(pos2(10.0, 20.0), vec2(400.0, 100.0));
        let rects = [
            PxRect::new(0, 360, 1920, 1200),
            PxRect::new(1920, 0, 2560, 1440),
            PxRect::new(-3840, 0, 3840, 2160),
        ];
        let out = layout_rects(&rects, area);
        assert_eq!(out.len(), 3);
        for r in &out {
            assert!(
                area.expand(0.01).contains_rect(*r),
                "{r:?} outside {area:?}"
            );
        }
        // Same scale for every screen, and the arrangement is kept.
        let s0 = out[0].width() / 1920.0;
        let s1 = out[1].width() / 2560.0;
        assert!((s0 - s1).abs() < 1e-4);
        assert!((out[0].height() / 1200.0 - s0).abs() < 1e-4);
        assert!(out[2].right() <= out[0].left() + 0.01);
        assert!((out[0].right() - out[1].left()).abs() < 0.01);
        assert!(layout_rects(&[], area).is_empty());
    }
}
