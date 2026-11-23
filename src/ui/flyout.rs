//! The quick panel ("flyout") that opens from the notification area icon: a
//! slider per monitor, "link all", presets and night light. The same panel
//! runs as its own small window (`Mode::Flyout`), as a child window of the
//! main window, and painted over an invented desktop for the screenshot.

use super::app::{logo_texture, paint_logo, toasts};
use super::core::Core;
use super::theme::{self, kelvin_color, pal, pal_ctx, regular, semibold, Icon};
use super::widgets::{self, Track};
use super::Options;
use crate::engine::UiRequest;
use crate::model::Connection;
use crate::settings::{KELVIN_MIN, KELVIN_NEUTRAL};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, Frame, Margin, Mesh, Rect, RichText, Sense,
    Shape, Stroke, TextureHandle, Ui,
};
use std::sync::atomic::Ordering;
use std::time::Instant;

pub const WIDTH: f32 = 360.0;

/// The primary display's scale (egui places new windows with it).
fn primary_scale_of(live: &crate::engine::LiveState) -> f32 {
    live.displays
        .iter()
        .find(|d| d.primary)
        .map(|d| d.scale_percent as f32 / 100.0)
        .unwrap_or(1.0)
}

fn primary_scale(core: &Core) -> f32 {
    primary_scale_of(&core.live)
}
const MARGIN: f32 = 16.0;
/// Space between the panel and the taskbar / screen edge.
const EDGE: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlyoutAction {
    OpenMain,
    OpenSettings,
    Close,
}

/// The panel's content. Returns what the user asked for, if anything.
pub fn panel(ui: &mut Ui, core: &mut Core, logo: Option<&TextureHandle>) -> Option<FlyoutAction> {
    let p = pal(ui);
    let mut action = None;
    ui.spacing_mut().item_spacing.y = 6.0;
    widgets::split_row(
        ui,
        32.0,
        |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            paint_logo(ui, logo, rect);
            ui.label(RichText::new("DisplayForge").font(semibold(15.5)));
            if core.demo {
                widgets::badge(ui, "DEMO", p.info);
            }
        },
        |ui| {
            if widgets::icon_button(ui, Icon::SETTINGS, "Settings").clicked() {
                action = Some(FlyoutAction::OpenSettings);
            }
            if widgets::icon_button(ui, Icon::OPEN_WINDOW, "Open DisplayForge").clicked() {
                action = Some(FlyoutAction::OpenMain);
            }
        },
    );
    ui.add_space(2.0);

    let live = core.live.clone();
    if !live.ready {
        widgets::hint(ui, "Looking for monitors…");
    } else if live.monitors.is_empty() {
        widgets::hint(
            ui,
            "No monitor answered. Check that DDC/CI is on in the monitor's own menu, then open DisplayForge and look again.",
        );
    }
    let min = core.settings.min_brightness();
    let linked = live
        .monitors
        .iter()
        .filter(|m| core.settings.prefs(&m.monitor.key).linked)
        .count();
    if core.settings.link_all && linked >= 2 {
        let mut v = core.link_value();
        slider_head(
            ui,
            Icon::LINK,
            "All linked monitors",
            &brightness_label(v),
            p.accent,
        );
        let w = ui.available_width();
        if widgets::level_slider(ui, &mut v, min..=100, 1, w, Track::Brightness, true).changed() {
            core.set_brightness(None, v);
        }
    }
    for m in &live.monitors {
        let prefs = core.settings.prefs(&m.monitor.key);
        if prefs.hidden {
            continue;
        }
        let mut v = core.brightness(m);
        let icon = match m.monitor.connection {
            Connection::Internal => Icon::LAPTOP,
            _ => Icon::MONITOR,
        };
        let name = core.monitor_name(m);
        let value_color = if v < 0 { p.info } else { p.text };
        slider_head(ui, icon, &name, &brightness_label(v), value_color);
        let w = ui.available_width();
        let r = widgets::level_slider(ui, &mut v, min..=100, 1, w, Track::Brightness, true);
        if r.changed() {
            core.set_brightness(Some(&m.monitor.key), v);
        }
        if let Some(err) = &m.error {
            ui.label(RichText::new(err).size(12.0).color(p.danger));
        }
    }

    ui.add_space(4.0);
    widgets::divider(ui);
    ui.add_space(2.0);
    // Presets.
    let presets: Vec<String> = core
        .settings
        .presets
        .iter()
        .map(|p| p.name.clone())
        .collect();
    if !presets.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for name in presets {
                let active = live.last_preset.as_deref() == Some(name.as_str());
                if widgets::chip(ui, &name, active).clicked() {
                    core.apply_preset(&name);
                }
            }
        });
        ui.add_space(2.0);
    }
    // Night light.
    let on = core.night_on();
    let moon = if on {
        kelvin_color(core.kelvin()).lerp_to_gamma(p.accent, 0.3)
    } else {
        p.muted
    };
    widgets::split_row(
        ui,
        30.0,
        |ui| {
            ui.label(RichText::new(Icon::MOON.glyph()).size(15.0).color(moon));
            ui.label(RichText::new("Night light").font(semibold(14.0)));
        },
        |ui| {
            if widgets::toggle(ui, on, true).clicked() {
                core.set_night(!on);
            }
        },
    );
    let mut k = core.kelvin() as i32;
    let r = widgets::slider_with_value(
        ui,
        &mut k,
        KELVIN_MIN as i32..=KELVIN_NEUTRAL as i32,
        50,
        Track::Kelvin,
        on,
        |v| format!("{v} K"),
    );
    if r.changed() {
        core.set_kelvin(k as u32);
    }
    // What is in charge.
    let status = status_line(core);
    if !status.is_empty() {
        ui.add_space(2.0);
        ui.label(RichText::new(status).size(12.0).color(p.muted));
    }
    action
}

fn brightness_label(v: i32) -> String {
    format!("{v} %")
}

/// A label row above a slider: icon, name and the value on the right.
fn slider_head(ui: &mut Ui, icon: Icon, name: &str, value: &str, value_color: Color32) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    ui.painter().text(
        rect.left_center() + vec2(8.0, 0.0),
        Align2::CENTER_CENTER,
        icon.glyph(),
        regular(14.0),
        p.muted,
    );
    let name_rect = Rect::from_min_max(
        pos2(rect.left() + 24.0, rect.top()),
        pos2(rect.right() - 60.0, rect.bottom()),
    );
    widgets::text_in(ui, name_rect, name, regular(14.0), p.text);
    ui.painter().text(
        rect.right_center(),
        Align2::RIGHT_CENTER,
        value,
        semibold(14.0),
        value_color,
    );
}

/// "Following the schedule · next 22:30", the active profile, or idle dimming.
pub fn status_line(core: &Core) -> String {
    let live = &core.live;
    let tz = core.tz();
    if let Some(app) = &live.active_profile {
        return format!("Profile for {app} is active");
    }
    if live.idle_dimmed {
        return "Dimmed while you are away".into();
    }
    let s = &live.schedule;
    if !s.enabled {
        return String::new();
    }
    if !s.following {
        return "Schedule paused after a manual change".into();
    }
    match &s.next {
        Some((t, _)) => format!(
            "Following the schedule  ·  next change {}",
            crate::timeutil::fmt_hm(*t, tz)
        ),
        None => "Following the schedule".into(),
    }
}

fn panel_frame(ctx: &egui::Context) -> Frame {
    let p = pal_ctx(ctx);
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .inner_margin(Margin::same(MARGIN as i8))
}

/// The bottom-right corner of the work area under the mouse, in physical
/// pixels, and that monitor's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Anchor {
    right: f32,
    bottom: f32,
    scale: f32,
}

fn anchor() -> Anchor {
    let (area, scale) = crate::sys::work_area_at_cursor();
    Anchor {
        right: area.right() as f32,
        bottom: area.bottom() as f32,
        scale: if scale > 0.1 { scale } else { 1.0 },
    }
}

/// The top-left corner of the panel in points of `points_scale` (egui places
/// a new window with the primary monitor's scale, and moves an existing one
/// with the scale of the monitor it is on).
fn panel_pos(a: Anchor, height: f32, points_scale: f32) -> egui::Pos2 {
    let s = if points_scale > 0.1 {
        points_scale
    } else {
        1.0
    };
    let x = a.right - (WIDTH + EDGE) * a.scale;
    let y = a.bottom - (height + EDGE) * a.scale;
    pos2(x / s, y / s)
}

/// The panel as a window of its own, or as a child window of the main window.
pub struct FlyoutWindow {
    pub open: bool,
    height: f32,
    placed_height: f32,
    focused_once: bool,
    opened: Option<Instant>,
    anchor: Option<Anchor>,
    start_pos: Option<egui::Pos2>,
}

impl Default for FlyoutWindow {
    fn default() -> Self {
        FlyoutWindow {
            open: false,
            height: 460.0,
            placed_height: 460.0,
            focused_once: false,
            opened: None,
            anchor: None,
            start_pos: None,
        }
    }
}

impl FlyoutWindow {
    pub fn new_open() -> Self {
        FlyoutWindow {
            open: true,
            ..FlyoutWindow::default()
        }
    }

    /// Paints the panel into the current viewport, keeps it sized to its
    /// content above the taskbar and tracks focus. Returns an action,
    /// `Close` when focus was lost or Esc was pressed.
    fn contents(
        &mut self,
        ctx: &egui::Context,
        core: &mut Core,
        logo: Option<&TextureHandle>,
    ) -> Option<FlyoutAction> {
        let opened = *self.opened.get_or_insert_with(Instant::now);
        let mut action = None;
        egui::CentralPanel::default()
            .frame(panel_frame(ctx))
            .show(ctx, |ui| {
                action = panel(ui, core, logo);
                let h = ui.min_rect().height() + 2.0 * MARGIN;
                if (h - self.height).abs() > 1.0 {
                    self.height = h;
                    ctx.request_repaint();
                }
            });
        if (self.height - self.placed_height).abs() > 1.0 {
            self.placed_height = self.height;
            if let Some(a) = self.anchor {
                let own = ctx.input(|i| i.viewport().native_pixels_per_point.unwrap_or(a.scale));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(WIDTH, self.height)));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(panel_pos(
                    a,
                    self.height,
                    own,
                )));
            }
        }
        let (focused, close, esc) = ctx.input(|i| {
            (
                i.viewport().focused,
                i.viewport().close_requested(),
                i.key_pressed(egui::Key::Escape),
            )
        });
        match focused {
            Some(true) => self.focused_once = true,
            Some(false) if self.focused_once => action = action.or(Some(FlyoutAction::Close)),
            _ => {
                if opened.elapsed().as_millis() < 1500 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
            }
        }
        if close || esc {
            action = Some(FlyoutAction::Close);
        }
        action
    }

    /// The window: placed once (later moves are sent as commands).
    fn builder(&mut self, primary_scale: f32) -> egui::ViewportBuilder {
        let a = *self.anchor.get_or_insert_with(anchor);
        let pos = *self
            .start_pos
            .get_or_insert_with(|| panel_pos(a, self.height, primary_scale));
        egui::ViewportBuilder::default()
            .with_title("DisplayForge")
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false)
            .with_inner_size([WIDTH, self.placed_height])
            .with_position(pos)
    }

    /// The panel as a child window of the main window.
    pub fn show_child(
        &mut self,
        ctx: &egui::Context,
        core: &mut Core,
        logo: Option<&TextureHandle>,
    ) -> Option<FlyoutAction> {
        let builder = self.builder(primary_scale(core));
        let mut action = None;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("flyout"),
            builder,
            |ctx, _| {
                action = self.contents(ctx, core, logo);
            },
        );
        if action.is_some() {
            *self = FlyoutWindow::default();
        }
        action
    }
}

/// The panel on its own (`Mode::Flyout`).
struct FlyoutApp {
    core: Core,
    window: FlyoutWindow,
    logo: Option<TextureHandle>,
    closing: bool,
}

pub fn run_flyout(options: Options, icon: egui::IconData) -> eframe::Result<()> {
    let shared = options.shared.clone().unwrap_or_else(crate::demo::shared);
    let mut window = FlyoutWindow::default();
    let scale = primary_scale_of(&shared.live());
    let builder = window.builder(scale).with_icon(icon.clone());
    let native = eframe::NativeOptions {
        viewport: builder,
        ..Default::default()
    };
    eframe::run_native(
        "DisplayForge",
        native,
        Box::new(move |cc| {
            let ctx = &cc.egui_ctx;
            theme::install(ctx);
            let core = Core::new(shared, options.control.clone());
            theme::apply_choice(ctx, core.settings.theme);
            let wake_ctx = ctx.clone();
            if let Ok(mut waker) = core.shared.waker.lock() {
                *waker = Some(Box::new(move || wake_ctx.request_repaint()));
            }
            Ok(Box::new(FlyoutApp {
                core,
                logo: logo_texture(ctx, &icon),
                window,
                closing: false,
            }))
        }),
    )
}

impl eframe::App for FlyoutApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.core.begin_frame();
        // A request (open the main window, quit) closes the panel; main.rs
        // takes the request once this window is gone.
        let pending = self
            .core
            .shared
            .requests
            .lock()
            .map(|r| !r.is_empty())
            .unwrap_or(false);
        if pending || self.core.shared.quit.load(Ordering::SeqCst) {
            self.closing = true;
        }
        if !self.closing {
            match self
                .window
                .contents(ctx, &mut self.core, self.logo.as_ref())
            {
                Some(FlyoutAction::OpenMain) => {
                    self.core.request(UiRequest::Open(None));
                    self.closing = true;
                }
                Some(FlyoutAction::OpenSettings) => {
                    self.core.request(UiRequest::Open(Some("settings".into())));
                    self.closing = true;
                }
                Some(FlyoutAction::Close) => self.closing = true,
                None => {}
            }
        }
        if self.closing {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        toasts(ctx, &self.core.toasts);
        self.core
            .toasts
            .retain(|t| t.born.elapsed().as_secs_f32() < 3.5);
        if self.core.end_frame() || self.core.holding() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.core.flush();
        if let Ok(mut waker) = self.core.shared.waker.lock() {
            *waker = None;
        }
    }
}

/// For the "flyout" screenshot: an invented desktop with a taskbar, and the
/// panel open above the notification area.
pub fn backdrop_shot(ctx: &egui::Context, core: &mut Core, logo: Option<&TextureHandle>) {
    let p = pal_ctx(ctx);
    egui::CentralPanel::default()
        .frame(Frame::new())
        .show(ctx, |ui| {
            let full = ui.max_rect();
            let painter = ui.painter();
            // Wallpaper: a soft dusk gradient.
            let (top, bottom) = if p.dark {
                (
                    Color32::from_rgb(0x14, 0x22, 0x52),
                    Color32::from_rgb(0x3a, 0x2c, 0x4a),
                )
            } else {
                (
                    Color32::from_rgb(0x9d, 0xb8, 0xee),
                    Color32::from_rgb(0xf6, 0xd6, 0xa8),
                )
            };
            let mut mesh = Mesh::default();
            mesh.colored_vertex(full.left_top(), top);
            mesh.colored_vertex(full.right_top(), top);
            mesh.colored_vertex(full.left_bottom(), bottom);
            mesh.colored_vertex(full.right_bottom(), bottom);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(1, 2, 3);
            painter.add(Shape::mesh(mesh));
            // Taskbar.
            let bar = Rect::from_min_max(pos2(full.left(), full.bottom() - 48.0), full.max);
            let bar_fill = if p.dark {
                Color32::from_rgb(0x1b, 0x1d, 0x26)
            } else {
                Color32::from_rgb(0xee, 0xf0, 0xf5)
            };
            painter.rect_filled(bar, 0, bar_fill);
            painter.hline(bar.x_range(), bar.top(), Stroke::new(1.0_f32, p.border));
            let cx = bar.center().x;
            for i in -3..=3 {
                let r = Rect::from_center_size(
                    pos2(cx + i as f32 * 44.0, bar.center().y),
                    vec2(26.0, 26.0),
                );
                let c = [
                    Color32::from_rgb(0x2f, 0x86, 0xc9),
                    Color32::from_rgb(0x4e, 0x9a, 0x3c),
                    Color32::from_rgb(0xd9, 0x6a, 0x2b),
                    Color32::from_rgb(0x7c, 0x5c, 0xd6),
                    Color32::from_rgb(0x10, 0xa3, 0x7f),
                    Color32::from_rgb(0xc2, 0x48, 0x7a),
                    Color32::from_rgb(0xb8, 0x8a, 0x1f),
                ][(i + 3) as usize];
                painter.rect_filled(r, CornerRadius::same(6), c.gamma_multiply(0.85));
            }
            // Notification area: our icon (highlighted), a few glyphs, the clock.
            let tz = core.tz();
            let now = core.now();
            let clock = crate::timeutil::fmt_hm(now, tz);
            let day = crate::timeutil::local_day(now, tz);
            let (y, m, d) = crate::timeutil::civil_from_days(day);
            let right = bar.right() - 14.0;
            painter.text(
                pos2(right, bar.center().y - 8.0),
                Align2::RIGHT_CENTER,
                clock,
                regular(12.0),
                p.text,
            );
            painter.text(
                pos2(right, bar.center().y + 9.0),
                Align2::RIGHT_CENTER,
                format!("{d:02}/{m:02}/{y}"),
                regular(12.0),
                p.text,
            );
            let mut x = right - 92.0;
            for icon in [Icon::VOLUME, Icon::WIFI] {
                painter.text(
                    pos2(x, bar.center().y),
                    Align2::CENTER_CENTER,
                    icon.glyph(),
                    regular(14.0),
                    p.text,
                );
                x -= 30.0;
            }
            let ours = Rect::from_center_size(pos2(x, bar.center().y), vec2(32.0, 36.0));
            painter.rect_filled(ours, CornerRadius::same(5), p.hover);
            paint_logo(
                ui,
                logo,
                Rect::from_center_size(ours.center(), vec2(18.0, 18.0)),
            );

            // The panel, right above the taskbar (its height is measured each frame).
            let id = egui::Id::new("flyout-shot");
            let height: f32 = ctx.data(|d| d.get_temp(id)).unwrap_or(560.0);
            let panel_pos = pos2(full.right() - WIDTH - EDGE, bar.top() - EDGE - height);
            let area = egui::Area::new(id)
                .fixed_pos(panel_pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    panel_frame(ctx)
                        .corner_radius(CornerRadius::same(10))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 8],
                            blur: 28,
                            spread: 0,
                            color: Color32::from_black_alpha(90),
                        })
                        .show(ui, |ui| {
                            ui.set_width(WIDTH - 2.0 * MARGIN);
                            let _ = panel(ui, core, logo);
                        });
                });
            let h = area.response.rect.height();
            if (h - height).abs() > 0.5 {
                ctx.data_mut(|d| d.insert_temp(id, h));
                ctx.request_repaint();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_sits_above_the_taskbar_at_the_right() {
        // A 1920 x 1040 work area at 150 %: the panel's bottom-right corner is
        // 12 points (18 px) in from the corner of the work area.
        let a = Anchor {
            right: 1920.0,
            bottom: 1040.0,
            scale: 1.5,
        };
        let pos = panel_pos(a, 400.0, 1.5);
        assert!((pos.x + WIDTH + EDGE - 1280.0).abs() < 0.01);
        assert!((pos.y + 400.0 + EDGE - 1040.0 / 1.5).abs() < 0.01);
        // Placed with a different (primary) scale, the physical spot is the same.
        let p1 = panel_pos(a, 400.0, 1.0);
        assert!((p1.x - pos.x * 1.5).abs() < 0.01);
    }
}
