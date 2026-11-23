//! The Display page: resolution, refresh rate, orientation and the main
//! display, with a "keep these settings?" countdown that reverts by itself,
//! and a picture of how the screens are arranged.

use super::app::{page_header, App};
use super::core::Core;
use super::theme::{pal, pal_ctx, regular, semibold, Icon};
use super::widgets::{self, LayoutItem};
use crate::model::{DisplayChange, DisplayMode, DisplayState, Orientation};
use crate::revert::{RevertTimer, CONFIRM_SECONDS};
use eframe::egui::{self, vec2, Align2, CornerRadius, RichText, Sense, Stroke, StrokeKind, Ui};
use std::collections::HashMap;

/// A change being made on one display, before "Apply".
#[derive(Clone, Copy, Debug, PartialEq)]
struct Edit {
    size: (u32, u32),
    refresh: u32,
    orientation: Orientation,
}

impl Edit {
    fn of(d: &DisplayState) -> Edit {
        Edit {
            size: (d.current.width, d.current.height),
            refresh: d.current.refresh,
            orientation: d.orientation,
        }
    }
}

/// The countdown after a change.
pub struct Confirm {
    device: String,
    name: String,
    timer: RevertTimer,
    /// What to go back to, once the engine has answered.
    before: Option<DisplayChange>,
    /// A countdown that stands still (screenshots).
    frozen: Option<u64>,
}

#[derive(Default)]
pub struct DisplayPage {
    edits: HashMap<String, Edit>,
    confirm: Option<Confirm>,
    selected: Option<usize>,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Whole seconds left (rounded up) of a countdown, 0 when it ran out.
pub fn seconds_left(started_ms: u64, seconds: u64, now_ms: u64) -> u64 {
    let end = started_ms + seconds * 1000;
    if now_ms >= end {
        0
    } else {
        (end - now_ms).div_ceil(1000)
    }
}

/// Distinct resolutions, largest first.
fn sizes(d: &DisplayState) -> Vec<(u32, u32)> {
    let mut v: Vec<(u32, u32)> = d.modes.iter().map(|m| (m.width, m.height)).collect();
    v.push((d.current.width, d.current.height));
    v.sort_by_key(|s| std::cmp::Reverse((s.0 * s.1, s.0)));
    v.dedup();
    v
}

/// Refresh rates offered at a resolution, highest first.
fn rates(d: &DisplayState, size: (u32, u32)) -> Vec<u32> {
    let mut v: Vec<u32> = d
        .modes
        .iter()
        .filter(|m| (m.width, m.height) == size)
        .map(|m| m.refresh)
        .collect();
    if size == (d.current.width, d.current.height) {
        v.push(d.current.refresh);
    }
    v.sort_by(|a, b| b.cmp(a));
    v.dedup();
    v
}

/// The change that turns the current state into `edit`.
fn change_for(d: &DisplayState, edit: &Edit) -> DisplayChange {
    let mode_changed =
        edit.size != (d.current.width, d.current.height) || edit.refresh != d.current.refresh;
    let mode = mode_changed.then(|| {
        d.modes
            .iter()
            .copied()
            .filter(|m| (m.width, m.height) == edit.size && m.refresh == edit.refresh)
            .max_by_key(|m| (m.bits == d.current.bits, m.bits))
            .unwrap_or(DisplayMode {
                width: edit.size.0,
                height: edit.size.1,
                refresh: edit.refresh,
                bits: d.current.bits,
            })
    });
    DisplayChange {
        mode,
        orientation: (edit.orientation != d.orientation).then_some(edit.orientation),
        make_primary: false,
    }
}

fn hdr_text(d: &DisplayState) -> String {
    match (d.hdr.supported, d.hdr.enabled, d.hdr.sdr_white_nits) {
        (_, true, Some(n)) => format!("On (SDR content at {n:.0} nits)"),
        (_, true, None) => "On".into(),
        (true, false, _) => "Supported, switched off".into(),
        _ => "Not supported".into(),
    }
}

impl DisplayPage {
    /// The "display-confirm" screenshot: the countdown just after a change.
    pub fn demo_confirm(&mut self, core: &Core, frozen: bool) {
        let Some(d) = core.live.displays.iter().find(|d| d.primary) else {
            return;
        };
        let mut before = d.current;
        before.refresh = rates(d, (d.current.width, d.current.height))
            .into_iter()
            .find(|r| *r != d.current.refresh)
            .unwrap_or(60);
        self.confirm = Some(Confirm {
            device: d.device.clone(),
            name: d.name.clone(),
            timer: RevertTimer::new(now_ms().saturating_sub(3000), CONFIRM_SECONDS),
            before: Some(DisplayChange {
                mode: Some(before),
                orientation: None,
                make_primary: false,
            }),
            frozen: frozen.then_some(CONFIRM_SECONDS - 3),
        });
    }

    fn start(&mut self, core: &mut Core, d: &DisplayState, change: DisplayChange) {
        if let Ok(mut live) = core.shared.live.lock() {
            live.display_result = None;
        }
        core.send(crate::engine::Cmd::ApplyDisplay {
            device: d.device.clone(),
            change,
        });
        self.edits.remove(&d.device);
        self.confirm = Some(Confirm {
            device: d.device.clone(),
            name: d.name.clone(),
            timer: RevertTimer::new(now_ms(), CONFIRM_SECONDS),
            before: None,
            frozen: None,
        });
    }

    fn revert(&mut self, core: &mut Core, why: &str) {
        if let Some(c) = self.confirm.take() {
            match c.before {
                Some(before) if !before.is_empty() => {
                    core.send(crate::engine::Cmd::ApplyDisplay {
                        device: c.device,
                        change: before,
                    });
                    core.toast(
                        format!("{why}: the previous display settings are back"),
                        false,
                    );
                }
                _ => core.toast(
                    "Windows did not report the previous settings, so nothing was reverted",
                    true,
                ),
            }
        }
    }

    /// Never leave a change unconfirmed when the window closes.
    pub fn revert_on_exit(&mut self, core: &mut Core) {
        if self.confirm.is_some() {
            self.revert(core, "Closed before confirming");
        }
    }
}

impl App {
    pub(super) fn display_ui(&mut self, ui: &mut Ui) {
        let displays = self.core.live.displays.clone();
        let subtitle = match displays.len() {
            0 => "No display found".to_string(),
            1 => "1 display: resolution, refresh rate and orientation".to_string(),
            n => {
                format!("{n} displays: resolution, refresh rate, orientation and the main display")
            }
        };
        page_header(ui, "Display", &subtitle, |_| {});
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let p = pal(ui);
                widgets::card(ui, |ui| {
                    widgets::split_row(
                        ui,
                        24.0,
                        |ui| widgets::section(ui, "Arrangement"),
                        |ui| {
                            ui.label(
                                RichText::new("P = main display. Arrange screens in Windows Settings > Display.")
                                    .size(12.5)
                                    .color(p.muted),
                            );
                        },
                    );
                    let items: Vec<LayoutItem> = displays
                        .iter()
                        .enumerate()
                        .map(|(i, d)| LayoutItem {
                            rect: d.rect,
                            number: i + 1,
                            name: d.name.clone(),
                            primary: d.primary,
                            selected: self.display.selected == Some(i),
                        })
                        .collect();
                    if let Some(i) = widgets::layout_picture(ui, &items, 150.0) {
                        self.display.selected = Some(i);
                    }
                });
                ui.add_space(widgets::GAP);
                let cols = if ui.available_width() >= 820.0 { 2 } else { 1 };
                let numbered: Vec<(usize, DisplayState)> =
                    displays.into_iter().enumerate().collect();
                for row in numbered.chunks(cols) {
                    let weights = vec![1.0; cols];
                    let mut start: Option<(DisplayState, DisplayChange)> = None;
                    let page = &mut self.display;
                    let core = &self.core;
                    widgets::card_row(ui, &weights, &mut |ui, i| {
                        if let Some((n, d)) = row.get(i) {
                            if let Some(c) = display_card(ui, page, core, *n, d) {
                                start = Some((d.clone(), c));
                            }
                        }
                    });
                    if let Some((d, change)) = start {
                        self.display.start(&mut self.core, &d, change);
                    }
                    ui.add_space(widgets::GAP);
                }
            });
    }

    /// The countdown dialog, and answers from the engine.
    pub(super) fn display_confirm_ui(&mut self, ctx: &egui::Context) {
        // The engine's answer to the last change.
        if let Some((device, result)) = self.core.live.display_result.clone() {
            if let Ok(mut live) = self.core.shared.live.lock() {
                live.display_result = None;
            }
            match (&mut self.display.confirm, result) {
                (Some(c), Ok(before)) if c.device == device && c.before.is_none() => {
                    c.before = Some(before)
                }
                (Some(c), Err(e)) if c.device == device => {
                    self.display.confirm = None;
                    self.core
                        .toast(format!("Could not change the display: {e}"), true);
                }
                (None, Err(e)) => self
                    .core
                    .toast(format!("Could not change the display: {e}"), true),
                _ => {}
            }
        }
        let Some(confirm) = &mut self.display.confirm else {
            return;
        };
        let now = now_ms();
        confirm.timer.tick(now);
        let left = confirm
            .frozen
            .unwrap_or_else(|| seconds_left(confirm.timer.started_ms, confirm.timer.seconds, now));
        let name = confirm.name.clone();
        let waiting = confirm.before.is_none();
        if left == 0 {
            self.display.revert(&mut self.core, "Time ran out");
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let p = pal_ctx(ctx);
        let mut keep = false;
        let mut revert = false;
        egui::Modal::new(egui::Id::new("display-confirm"))
            .frame(widgets::modal_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
                    let f = left as f32 / CONFIRM_SECONDS as f32;
                    ui.painter().circle_stroke(
                        rect.center(),
                        23.0,
                        Stroke::new(4.0_f32, p.hover),
                    );
                    let n = 48;
                    let pts: Vec<egui::Pos2> = (0..=((n as f32 * f) as usize))
                        .map(|i| {
                            let a = -std::f32::consts::FRAC_PI_2
                                + i as f32 / n as f32 * std::f32::consts::TAU;
                            rect.center() + vec2(a.cos(), a.sin()) * 23.0
                        })
                        .collect();
                    if pts.len() > 1 {
                        ui.painter()
                            .add(egui::Shape::line(pts, Stroke::new(4.0_f32, p.accent)));
                    }
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        left.to_string(),
                        semibold(18.0),
                        p.text,
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(
                            RichText::new("Keep these display settings?").font(semibold(18.0)),
                        );
                        ui.label(
                            RichText::new(format!("Reverting in {left} s"))
                                .color(p.accent)
                                .font(semibold(14.0)),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{name} changed. If the picture looks wrong or the screen stays black, just wait: the previous settings come back by themselves."
                        ))
                        .color(p.muted),
                    )
                    .wrap(),
                );
                if waiting {
                    ui.label(
                        RichText::new("Waiting for Windows to apply the change…")
                            .size(12.5)
                            .color(p.faint),
                    );
                }
                ui.add_space(10.0);
                widgets::button_row(ui, |ui| {
                    if widgets::primary_button(ui, &Icon::CHECK.with("Keep"), true).clicked() {
                        keep = true;
                    }
                    if widgets::ghost_button(ui, &Icon::UNDO.with("Revert")).clicked() {
                        revert = true;
                    }
                });
            });
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if keep {
            if let Some(c) = &mut self.display.confirm {
                c.timer.confirm();
            }
            self.display.confirm = None;
            self.core.toast("Kept the new display settings", false);
        } else if revert || esc {
            if let Some(c) = &mut self.display.confirm {
                c.timer.revert();
            }
            self.display.revert(&mut self.core, "Reverted");
        }
    }
}

fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(118.0, 28.0), Sense::hover());
        ui.painter().text(
            rect.left_center(),
            Align2::LEFT_CENTER,
            label,
            regular(14.0),
            p.muted,
        );
        add(ui);
    });
}

/// One display's card. Returns a change to apply.
fn display_card(
    ui: &mut Ui,
    page: &mut DisplayPage,
    core: &Core,
    index: usize,
    d: &DisplayState,
) -> Option<DisplayChange> {
    let p = pal(ui);
    let mut apply = None;
    let monitor = core
        .live
        .monitors
        .iter()
        .find(|m| m.monitor.device == d.device);
    let native = monitor
        .and_then(|m| m.monitor.edid.as_ref())
        .and_then(|e| e.native)
        .map(|(w, h, _)| (w, h));
    let selected = page.selected == Some(index);
    // Head: a little screen with the display's proportions.
    ui.horizontal(|ui| {
        let (head, _) = ui.allocate_exact_size(vec2(74.0, 52.0), Sense::hover());
        let (w, h) = (d.rect.w.max(1) as f32, d.rect.h.max(1) as f32);
        let aspect = w / h;
        let size = if aspect >= 1.0 {
            vec2(66.0, (66.0 / aspect).max(20.0))
        } else {
            vec2((44.0 * aspect).max(20.0), 44.0)
        };
        let screen = egui::Rect::from_center_size(head.center() - vec2(0.0, 3.0), size);
        ui.painter().rect(
            screen,
            CornerRadius::same(4),
            if d.primary || selected {
                p.selected
            } else {
                p.hover
            },
            Stroke::new(
                1.5_f32,
                if d.primary || selected {
                    p.accent
                } else {
                    p.faint
                },
            ),
            StrokeKind::Inside,
        );
        ui.painter().line_segment(
            [
                egui::pos2(head.center().x, screen.bottom()),
                egui::pos2(head.center().x, screen.bottom() + 5.0),
            ],
            Stroke::new(1.5_f32, p.faint),
        );
        ui.painter().text(
            screen.center(),
            Align2::CENTER_CENTER,
            (index + 1).to_string(),
            semibold(16.0),
            p.text,
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.label(RichText::new(&d.name).font(semibold(16.0)));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                if d.primary {
                    widgets::badge(ui, "Main display", p.accent);
                }
                widgets::badge(ui, &d.current.label(), p.muted);
                widgets::badge(ui, &format!("{} Hz", d.current.refresh), p.muted);
                if d.hdr.enabled {
                    widgets::badge(ui, "HDR", p.info);
                }
            });
        });
    });
    ui.add_space(2.0);
    let current = Edit::of(d);
    let mut edit = page.edits.get(&d.device).copied().unwrap_or(current);
    let size_list = sizes(d);
    row(ui, "Resolution", |ui| {
        let label = |s: (u32, u32)| {
            let mut t = format!("{} x {}", s.0, s.1);
            if Some(s) == native {
                t.push_str("  (native)");
            }
            t
        };
        egui::ComboBox::from_id_salt(("res", &d.device))
            .width(220.0)
            .selected_text(label(edit.size))
            .show_ui(ui, |ui| {
                for s in &size_list {
                    if ui.selectable_label(edit.size == *s, label(*s)).clicked() {
                        edit.size = *s;
                        let r = rates(d, *s);
                        if !r.contains(&edit.refresh) {
                            edit.refresh = r.first().copied().unwrap_or(60);
                        }
                    }
                }
            });
    });
    let rate_list = rates(d, edit.size);
    row(ui, "Refresh rate", |ui| {
        egui::ComboBox::from_id_salt(("hz", &d.device))
            .width(220.0)
            .selected_text(format!("{} Hz", edit.refresh))
            .show_ui(ui, |ui| {
                for r in &rate_list {
                    if ui
                        .selectable_label(edit.refresh == *r, format!("{r} Hz"))
                        .clicked()
                    {
                        edit.refresh = *r;
                    }
                }
            });
    });
    row(ui, "Orientation", |ui| {
        egui::ComboBox::from_id_salt(("rot", &d.device))
            .width(220.0)
            .selected_text(edit.orientation.label())
            .show_ui(ui, |ui| {
                for o in Orientation::ALL {
                    if ui
                        .selectable_label(edit.orientation == o, o.label())
                        .clicked()
                    {
                        edit.orientation = o;
                    }
                }
            });
    });
    row(ui, "Scaling", |ui| {
        ui.label(format!("{} %", d.scale_percent));
        ui.label(
            RichText::new("set in Windows Settings")
                .size(12.5)
                .color(p.faint),
        );
    });
    row(ui, "HDR", |ui| {
        ui.label(hdr_text(d));
    });
    row(ui, "Position", |ui| {
        ui.label(format!(
            "{}, {}  ·  {} x {} pixels",
            d.rect.x, d.rect.y, d.rect.w, d.rect.h
        ));
    });
    if edit == current {
        page.edits.remove(&d.device);
    } else {
        page.edits.insert(d.device.clone(), edit);
    }
    let changed = edit != current;
    let confirm_open = page.confirm.is_some();
    ui.add_space(2.0);
    widgets::button_row(ui, |ui| {
        if widgets::primary_button(ui, "Apply", changed && !confirm_open)
            .on_hover_text(
                "You get 15 seconds to keep the change; otherwise it is undone by itself",
            )
            .clicked()
        {
            apply = Some(change_for(d, &edit));
        }
        if changed && widgets::ghost_button(ui, "Undo").clicked() {
            page.edits.remove(&d.device);
        }
        if !d.primary
            && widgets::ghost_button_enabled(ui, "Make main display", !confirm_open)
                .on_hover_text("The taskbar clock and new windows go to the main display")
                .clicked()
        {
            apply = Some(DisplayChange {
                make_primary: true,
                ..DisplayChange::default()
            });
        }
    });
    apply
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_rounds_up_and_stops_at_zero() {
        assert_eq!(seconds_left(1000, 15, 1000), 15);
        assert_eq!(seconds_left(1000, 15, 1001), 15);
        assert_eq!(seconds_left(1000, 15, 4000), 12);
        assert_eq!(seconds_left(1000, 15, 15_999), 1);
        assert_eq!(seconds_left(1000, 15, 16_000), 0);
        assert_eq!(seconds_left(1000, 15, 99_000), 0);
    }

    #[test]
    fn changes_only_hold_what_differs() {
        let d = crate::demo::displays().remove(1);
        let mut e = Edit::of(&d);
        assert!(change_for(&d, &e).is_empty());
        e.refresh = 60;
        let c = change_for(&d, &e);
        assert_eq!(c.mode.map(|m| m.refresh), Some(60));
        assert_eq!(c.orientation, None);
        e = Edit::of(&d);
        e.orientation = Orientation::Portrait;
        let c = change_for(&d, &e);
        assert!(c.mode.is_none());
        assert_eq!(c.orientation, Some(Orientation::Portrait));
        let s = sizes(&d);
        assert_eq!(s[0], (2560, 1440));
        assert!(rates(&d, (2560, 1440)).starts_with(&[144]));
    }
}
