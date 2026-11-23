//! The Tools page: full-screen test patterns on a chosen monitor, "identify
//! monitors" and the arrangement. Patterns and numbers are borderless,
//! always-on-top child windows that close with Esc, a click or a timer.

use super::app::{page_header, App};
use super::core::Core;
use super::theme::{pal, regular, semibold, Icon};
use super::widgets::{self, LayoutItem};
use crate::model::PxRect;
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Mesh, Painter, Rect, RichText, Sense,
    Shape, Stroke, Ui,
};
use std::time::Instant;

/// One full-screen test image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slide {
    Solid(Color32, &'static str),
    Gradient,
    Bars,
    Sharpness,
}

impl Slide {
    pub fn name(self) -> String {
        match self {
            Slide::Solid(_, n) => n.to_string(),
            Slide::Gradient => "Gradients (banding)".into(),
            Slide::Bars => "Colour bars".into(),
            Slide::Sharpness => "Sharpness and text".into(),
        }
    }
}

const fn grey(v: u8) -> Color32 {
    Color32::from_rgb(v, v, v)
}

/// Every slide, in order: groups start at the indexes in [`GROUPS`].
pub const SLIDES: [Slide; 15] = [
    Slide::Solid(Color32::BLACK, "Black"),
    Slide::Solid(Color32::WHITE, "White"),
    Slide::Solid(Color32::from_rgb(255, 0, 0), "Red"),
    Slide::Solid(Color32::from_rgb(0, 255, 0), "Green"),
    Slide::Solid(Color32::from_rgb(0, 0, 255), "Blue"),
    Slide::Solid(Color32::from_rgb(0, 255, 255), "Cyan"),
    Slide::Solid(Color32::from_rgb(255, 0, 255), "Magenta"),
    Slide::Solid(Color32::from_rgb(255, 255, 0), "Yellow"),
    Slide::Solid(grey(128), "Grey 50 %"),
    Slide::Solid(grey(64), "Grey 25 %"),
    Slide::Solid(grey(13), "Near black 5 %"),
    Slide::Solid(grey(230), "Light grey 90 %"),
    Slide::Gradient,
    Slide::Bars,
    Slide::Sharpness,
];

/// (first slide, title, what it shows)
pub const GROUPS: [(usize, &str, &str); 5] = [
    (
        0,
        "Dead and stuck pixels",
        "Solid black, white and colours: a dot that stays lit or dark is a faulty pixel.",
    ),
    (
        8,
        "Uniformity",
        "Even greys: look for clouding, light bleeding at the edges and tinted corners.",
    ),
    (
        12,
        "Gradient banding",
        "Smooth ramps of grey, red, green and blue should show no steps.",
    ),
    (
        13,
        "Colour bars",
        "Saturated bars for a quick check of colour and brightness.",
    ),
    (
        14,
        "Sharpness and text",
        "Fine lines and small text: blur or colour fringes show scaling or sharpness issues.",
    ),
];

/// The group a slide belongs to.
pub fn group_of(slide: usize) -> usize {
    GROUPS
        .iter()
        .rposition(|(first, _, _)| *first <= slide)
        .unwrap_or(0)
}

/// The next / previous slide, wrapping around.
pub fn step_slide(index: usize, forward: bool) -> usize {
    let n = SLIDES.len();
    if forward {
        (index + 1) % n
    } else {
        (index + n - 1) % n
    }
}

/// Paints a slide over `rect`; `scale` shrinks line spacing and text for previews.
pub fn paint_slide(painter: &Painter, rect: Rect, slide: Slide, scale: f32) {
    match slide {
        Slide::Solid(c, _) => {
            painter.rect_filled(rect, 0, c);
        }
        Slide::Gradient => {
            let rows: [Color32; 4] = [
                Color32::WHITE,
                Color32::from_rgb(255, 0, 0),
                Color32::from_rgb(0, 255, 0),
                Color32::from_rgb(0, 0, 255),
            ];
            let h = rect.height() / rows.len() as f32;
            for (i, c) in rows.iter().enumerate() {
                let r = Rect::from_min_size(
                    pos2(rect.left(), rect.top() + h * i as f32),
                    vec2(rect.width(), h),
                );
                let mut mesh = Mesh::default();
                mesh.colored_vertex(r.left_top(), Color32::BLACK);
                mesh.colored_vertex(r.right_top(), *c);
                mesh.colored_vertex(r.left_bottom(), Color32::BLACK);
                mesh.colored_vertex(r.right_bottom(), *c);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(1, 2, 3);
                painter.add(Shape::mesh(mesh));
            }
        }
        Slide::Bars => {
            let bars = [
                Color32::WHITE,
                Color32::from_rgb(255, 255, 0),
                Color32::from_rgb(0, 255, 255),
                Color32::from_rgb(0, 255, 0),
                Color32::from_rgb(255, 0, 255),
                Color32::from_rgb(255, 0, 0),
                Color32::from_rgb(0, 0, 255),
                Color32::BLACK,
            ];
            let w = rect.width() / bars.len() as f32;
            let top_h = rect.height() * 0.75;
            for (i, c) in bars.iter().enumerate() {
                let r = Rect::from_min_size(
                    pos2(rect.left() + w * i as f32, rect.top()),
                    vec2(w + 0.5, top_h),
                );
                painter.rect_filled(r, 0, *c);
            }
            // A grey step scale underneath.
            let steps = 11;
            let sw = rect.width() / steps as f32;
            for i in 0..steps {
                let v = (i as f32 / (steps - 1) as f32 * 255.0) as u8;
                let r = Rect::from_min_max(
                    pos2(rect.left() + sw * i as f32, rect.top() + top_h),
                    pos2(rect.left() + sw * (i + 1) as f32 + 0.5, rect.bottom()),
                );
                painter.rect_filled(r, 0, grey(v));
            }
        }
        Slide::Sharpness => {
            painter.rect_filled(rect, 0, Color32::BLACK);
            let step = (16.0 * scale).max(3.0);
            let line = Stroke::new(1.0_f32, grey(200));
            let mut x = rect.left() + step;
            while x < rect.right() {
                painter.vline(x.round() + 0.5, rect.y_range(), line);
                x += step;
            }
            let mut y = rect.top() + step;
            while y < rect.bottom() {
                painter.hline(rect.x_range(), y.round() + 0.5, line);
                y += step;
            }
            // Text samples on white in the middle.
            let panel = Rect::from_center_size(
                rect.center(),
                vec2(rect.width() * 0.6, rect.height() * 0.5),
            );
            painter.rect_filled(panel, 0, Color32::WHITE);
            let mut ty = panel.top() + 12.0 * scale;
            for size in [9.0, 11.0, 13.0, 16.0, 20.0, 28.0] {
                let s = (size * scale).max(2.0);
                painter.text(
                    pos2(panel.left() + 14.0 * scale, ty),
                    Align2::LEFT_TOP,
                    "The quick brown fox jumps over the lazy dog 0123456789",
                    FontId::proportional(s),
                    Color32::BLACK,
                );
                ty += s * 1.6;
            }
            // One-pixel checkerboard strips: should look evenly grey.
            let strip = Rect::from_min_size(
                pos2(panel.left() + 14.0 * scale, panel.bottom() - 40.0 * scale),
                vec2(panel.width() - 28.0 * scale, 24.0 * scale),
            );
            if scale >= 1.0 {
                let mut mesh = Mesh::default();
                let (x0, y0) = (strip.left().round(), strip.top().round());
                for yy in 0..strip.height() as i32 {
                    for xx in (0..strip.width() as i32).step_by(2) {
                        let off = (yy % 2) as f32;
                        let px = Rect::from_min_size(
                            pos2(x0 + xx as f32 + off, y0 + yy as f32),
                            vec2(1.0, 1.0),
                        );
                        mesh.add_colored_rect(px, Color32::BLACK);
                    }
                }
                painter.add(Shape::mesh(mesh));
            } else {
                painter.rect_filled(strip, 0, grey(128));
            }
        }
    }
}

/// Test patterns running on one monitor.
struct PatternRun {
    rect: PxRect,
    index: usize,
    started: Instant,
    last_input: Instant,
    fullscreen_sent: bool,
    focused_once: bool,
}

#[derive(Default)]
pub struct ToolsPage {
    monitor: usize,
    group: usize,
    run: Option<PatternRun>,
    identify_until: Option<Instant>,
}

impl ToolsPage {
    pub fn identify_start(&mut self) {
        self.identify_until = Some(Instant::now() + std::time::Duration::from_secs(3));
    }
}

/// Converts a physical-pixel position to the points a new window is placed in
/// (egui places new windows using the primary monitor's scale).
fn to_points(x: f32, y: f32, primary_scale: f32) -> egui::Pos2 {
    let s = if primary_scale > 0.1 {
        primary_scale
    } else {
        1.0
    };
    pos2(x / s, y / s)
}

fn primary_scale(core: &Core) -> f32 {
    core.live
        .displays
        .iter()
        .find(|d| d.primary)
        .map(|d| d.scale_percent as f32 / 100.0)
        .unwrap_or(1.0)
}

impl App {
    pub(super) fn tools_ui(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "Tools",
            "Test patterns, monitor numbers and the arrangement",
            |_| {},
        );
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let core = &self.core;
                let page = &mut self.tools;
                let mut start = false;
                widgets::card_row(ui, &[1.45, 1.0], &mut |ui, i| {
                    if i == 0 {
                        start |= patterns_card(ui, core, page);
                    } else {
                        identify_card(ui, core, page);
                    }
                });
                if start {
                    self.start_patterns();
                }
            });
    }

    fn start_patterns(&mut self) {
        let monitors = &self.core.live.monitors;
        let Some(m) = monitors.get(self.tools.monitor).or(monitors.first()) else {
            self.core.toast("No monitor to show the patterns on", true);
            return;
        };
        let now = Instant::now();
        self.tools.run = Some(PatternRun {
            rect: m.monitor.rect,
            index: GROUPS[self.tools.group.min(GROUPS.len() - 1)].0,
            started: now,
            last_input: now,
            fullscreen_sent: false,
            focused_once: false,
        });
    }

    /// The pattern and identify windows; call every frame.
    pub(super) fn tools_windows(&mut self, ctx: &egui::Context) {
        let scale = primary_scale(&self.core);
        let demo = self.core.demo;
        // In the demo the invented monitors are not where real ones are:
        // show everything on the screen the window is on.
        let window_center = ctx.input(|i| i.viewport().outer_rect.map(|r| r.center()));
        let native = ctx.input(|i| i.viewport().native_pixels_per_point.unwrap_or(1.0));

        if let Some(run) = &mut self.tools.run {
            let center = if demo {
                window_center
                    .map(|c| pos2(c.x * native / scale, c.y * native / scale))
                    .unwrap_or(pos2(400.0, 300.0))
            } else {
                to_points(
                    run.rect.x as f32 + run.rect.w as f32 / 2.0,
                    run.rect.y as f32 + run.rect.h as f32 / 2.0,
                    scale,
                )
            };
            let builder = egui::ViewportBuilder::default()
                .with_title("DisplayForge test pattern")
                .with_decorations(false)
                .with_always_on_top()
                .with_taskbar(false)
                .with_position(center - vec2(200.0, 150.0))
                .with_inner_size([400.0, 300.0]);
            let mut close = false;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("test-pattern"),
                builder,
                |ctx, _| close = pattern_window(ctx, run),
            );
            if close {
                self.tools.run = None;
            }
        }

        if let Some(until) = self.tools.identify_until {
            if Instant::now() >= until {
                self.tools.identify_until = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                let monitors = self.core.live.monitors.clone();
                let mut close = false;
                for (i, m) in monitors.iter().enumerate() {
                    let size = vec2(240.0, 200.0);
                    let center = if demo {
                        let c = window_center
                            .map(|c| pos2(c.x * native / scale, c.y * native / scale))
                            .unwrap_or(pos2(500.0, 400.0));
                        let n = monitors.len() as f32;
                        c + vec2((i as f32 - (n - 1.0) / 2.0) * 260.0, 0.0)
                    } else {
                        to_points(
                            m.monitor.rect.x as f32 + m.monitor.rect.w as f32 / 2.0,
                            m.monitor.rect.y as f32 + m.monitor.rect.h as f32 / 2.0,
                            scale,
                        )
                    };
                    let builder = egui::ViewportBuilder::default()
                        .with_title("DisplayForge")
                        .with_decorations(false)
                        .with_always_on_top()
                        .with_taskbar(false)
                        .with_resizable(false)
                        .with_position(center - size / 2.0)
                        .with_inner_size(size);
                    let number = m.monitor.number;
                    let name = self.core.monitor_name(m);
                    let primary = m.monitor.primary;
                    ctx.show_viewport_immediate(
                        egui::ViewportId::from_hash_of(("identify", i)),
                        builder,
                        |ctx, _| {
                            close |= identify_window(ctx, number, &name, primary);
                        },
                    );
                }
                if close {
                    self.tools.identify_until = None;
                }
            }
        }
    }
}

/// Paints a test pattern window. Returns true when it should close.
fn pattern_window(ctx: &egui::Context, run: &mut PatternRun) -> bool {
    if !run.fullscreen_sent {
        run.fullscreen_sent = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    let mut close = false;
    let (esc, next, prev, moved, focused, close_req) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::ArrowRight)
                || i.key_pressed(egui::Key::ArrowDown)
                || i.key_pressed(egui::Key::Space)
                || i.key_pressed(egui::Key::PageDown)
                || i.pointer.button_clicked(egui::PointerButton::Primary),
            i.key_pressed(egui::Key::ArrowLeft)
                || i.key_pressed(egui::Key::ArrowUp)
                || i.key_pressed(egui::Key::PageUp)
                || i.pointer.button_clicked(egui::PointerButton::Secondary),
            i.pointer.delta().length() > 0.5,
            i.viewport().focused,
            i.viewport().close_requested(),
        )
    });
    if esc || close_req {
        close = true;
    }
    match focused {
        Some(true) => run.focused_once = true,
        Some(false) if run.focused_once => close = true,
        _ => {}
    }
    if next {
        run.index = step_slide(run.index, true);
    }
    if prev {
        run.index = step_slide(run.index, false);
    }
    if next || prev || moved {
        run.last_input = Instant::now();
    }
    let slide = SLIDES[run.index];
    egui::CentralPanel::default()
        .frame(egui::Frame::new())
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            paint_slide(ui.painter(), rect, slide, 1.0);
            // A hint that fades after a few seconds and comes back on movement.
            let idle = run.last_input.elapsed().as_secs_f32();
            let alpha = if idle < 3.0 {
                1.0
            } else {
                (1.0 - (idle - 3.0)).max(0.0)
            };
            if alpha > 0.0 {
                let text = format!(
                    "{}: {}  ·  {} of {}  ·  Click or arrow keys for the next, Esc to close",
                    GROUPS[group_of(run.index)].1,
                    slide.name(),
                    run.index + 1,
                    SLIDES.len()
                );
                let g = ui.painter().layout_no_wrap(
                    text,
                    regular(15.0),
                    Color32::from_white_alpha((255.0 * alpha) as u8),
                );
                let pill = Rect::from_center_size(
                    pos2(rect.center().x, rect.bottom() - 48.0),
                    g.size() + vec2(28.0, 16.0),
                );
                ui.painter().rect_filled(
                    pill,
                    CornerRadius::same(18),
                    Color32::from_black_alpha((180.0 * alpha) as u8),
                );
                ui.painter()
                    .galley(pill.center() - g.size() / 2.0, g, Color32::WHITE);
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            } else {
                ctx.set_cursor_icon(egui::CursorIcon::None);
            }
        });
    // Safety net: patterns never outstay an hour.
    if run.started.elapsed().as_secs() > 3600 {
        close = true;
    }
    close
}

/// Paints a big monitor number. Returns true when clicked or Esc.
fn identify_window(ctx: &egui::Context, number: usize, name: &str, primary: bool) -> bool {
    let accent = super::theme::SUNFLOWER;
    let mut close = false;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(Color32::from_rgb(0x12, 0x16, 0x2a)))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let resp = ui.interact(rect, ui.id().with("identify"), Sense::click());
            close |= resp.clicked();
            ui.painter().rect_stroke(
                rect.shrink(2.0),
                CornerRadius::same(4),
                Stroke::new(4.0_f32, accent),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center() - vec2(0.0, 18.0),
                Align2::CENTER_CENTER,
                number.to_string(),
                semibold(96.0),
                Color32::WHITE,
            );
            let label = if primary {
                format!("{name}  ·  main")
            } else {
                name.to_string()
            };
            ui.painter().text(
                pos2(rect.center().x, rect.bottom() - 30.0),
                Align2::CENTER_CENTER,
                label,
                regular(15.0),
                accent,
            );
        });
    close | ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.viewport().close_requested())
}

fn patterns_card(ui: &mut Ui, core: &Core, page: &mut ToolsPage) -> bool {
    let p = pal(ui);
    let mut start = false;
    widgets::section_icon(ui, Icon::COLOR, "Test patterns");
    widgets::hint(
        ui,
        "Full-screen images on one monitor. Click or use the arrow keys for the next one; Esc closes.",
    );
    // Which monitor.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        ui.label(RichText::new("On").color(p.muted));
        for (i, m) in core.live.monitors.iter().enumerate() {
            let label = format!("{}. {}", m.monitor.number, core.monitor_name(m));
            if widgets::chip(ui, &label, page.monitor == i).clicked() {
                page.monitor = i;
            }
        }
    });
    ui.add_space(2.0);
    for (gi, (first, title, text)) in GROUPS.iter().enumerate() {
        let selected = page.group == gi;
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
        if selected {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(9), p.selected);
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(9),
                Stroke::new(1.0_f32, p.accent.gamma_multiply(0.6)),
                egui::StrokeKind::Inside,
            );
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(9), p.hover);
        }
        let thumb = Rect::from_min_size(rect.left_top() + vec2(8.0, 7.0), vec2(64.0, 40.0));
        let painter = ui.painter_at(thumb);
        // Preview: the group's slides side by side for solids, else the slide itself.
        let next = GROUPS.get(gi + 1).map(|g| g.0).unwrap_or(SLIDES.len());
        let count = next - first;
        if count > 1 {
            let w = thumb.width() / count as f32;
            for (k, s) in SLIDES[*first..next].iter().enumerate() {
                let r = Rect::from_min_size(
                    pos2(thumb.left() + w * k as f32, thumb.top()),
                    vec2(w + 0.5, thumb.height()),
                );
                paint_slide(&painter, r, *s, 0.25);
            }
        } else {
            paint_slide(&painter, thumb, SLIDES[*first], 0.25);
        }
        ui.painter().rect_stroke(
            thumb,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, p.border),
            egui::StrokeKind::Outside,
        );
        let tx = thumb.right() + 12.0;
        ui.painter().text(
            pos2(tx, rect.top() + 9.0),
            Align2::LEFT_TOP,
            *title,
            semibold(14.0),
            p.text,
        );
        let g = widgets::elided(ui, text, regular(12.5), p.muted, rect.right() - tx - 8.0);
        ui.painter().galley(pos2(tx, rect.top() + 29.0), g, p.muted);
        if resp.clicked() {
            page.group = gi;
        }
        if resp.double_clicked() {
            page.group = gi;
            start = true;
        }
    }
    ui.add_space(4.0);
    let running = page.run.is_some();
    widgets::button_row(ui, |ui| {
        if widgets::primary_button(ui, &Icon::PLAY.with("Show full screen"), !running).clicked() {
            start = true;
        }
    });
    start
}

fn identify_card(ui: &mut Ui, core: &Core, page: &mut ToolsPage) {
    widgets::section_icon(ui, Icon::MONITOR, "Identify monitors");
    widgets::hint(
        ui,
        "Shows each monitor's number on its screen for three seconds, to match the names here to the screens on your desk.",
    );
    widgets::button_row(ui, |ui| {
        if widgets::primary_button(ui, "Identify", page.identify_until.is_none()).clicked() {
            page.identify_start();
        }
    });
    ui.add_space(4.0);
    widgets::divider(ui);
    ui.add_space(4.0);
    widgets::section(ui, "Arrangement");
    let items: Vec<LayoutItem> = core
        .live
        .monitors
        .iter()
        .enumerate()
        .map(|(i, m)| LayoutItem {
            rect: m.monitor.rect,
            number: m.monitor.number,
            name: core.monitor_name(m),
            primary: m.monitor.primary,
            selected: page.monitor == i,
        })
        .collect();
    if let Some(i) = widgets::layout_picture(ui, &items, 170.0) {
        page.monitor = i;
    }
    let p = pal(ui);
    ui.label(
        RichText::new("Click a screen to show the test patterns there.")
            .size(12.5)
            .color(p.muted),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slides_cycle_and_belong_to_groups() {
        assert_eq!(step_slide(0, true), 1);
        assert_eq!(step_slide(SLIDES.len() - 1, true), 0);
        assert_eq!(step_slide(0, false), SLIDES.len() - 1);
        assert_eq!(group_of(0), 0);
        assert_eq!(group_of(7), 0);
        assert_eq!(group_of(8), 1);
        assert_eq!(group_of(14), 4);
        for (first, _, _) in GROUPS {
            assert!(first < SLIDES.len());
        }
        assert_eq!(to_points(2400.0, 1200.0, 1.25), pos2(1920.0, 960.0));
        assert_eq!(to_points(100.0, 50.0, 0.0), pos2(100.0, 50.0));
    }
}
