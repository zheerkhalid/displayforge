//! The Monitors page (home): "link all", presets, night light and one card
//! per monitor with everything it supports.

use super::app::{page_header, App, Tab};
use super::core::Core;
use super::theme::{kelvin_color, pal, regular, semibold, Icon};
use super::widgets::{self, Track};
use crate::engine::{Cmd, MonitorView};
use crate::model::{color_preset_name, input_name, power_name, Connection, Feature};
use crate::settings::{DimMethod, KELVIN_MIN, KELVIN_NEUTRAL};
use eframe::egui::{
    self, vec2, Align2, Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui,
};

/// Width of the label column in feature rows.
const LABEL_W: f32 = 104.0;

pub fn connection_color(ui: &Ui, c: Connection) -> Color32 {
    let p = pal(ui);
    match c {
        Connection::DdcCi => p.good,
        Connection::Internal => p.info,
        Connection::SoftwareOnly => p.warn,
    }
}

pub fn connection_hint(c: Connection) -> &'static str {
    match c {
        Connection::DdcCi => "The monitor's own settings are changed over the display cable (DDC/CI).",
        Connection::Internal => "A built-in laptop screen, set through Windows.",
        Connection::SoftwareOnly => "This screen does not take commands over the cable. DisplayForge dims it in software (gamma or a dark overlay) and applies night light.",
    }
}

pub fn monitor_icon(c: Connection) -> Icon {
    match c {
        Connection::Internal => Icon::LAPTOP,
        _ => Icon::MONITOR,
    }
}

/// "since 16:21 (sunset - 30 min)" from "Since 16:21 (sunset - 30 min)".
pub fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

impl App {
    pub(super) fn monitors_ui(&mut self, ui: &mut Ui) {
        let count = self.core.live.monitors.len();
        let subtitle = if !self.core.live.ready {
            "Looking for monitors…".to_string()
        } else {
            match count {
                0 => "No monitor found".to_string(),
                1 => "1 monitor: brightness, contrast and more".to_string(),
                n => format!("{n} monitors: brightness, contrast and more"),
            }
        };
        let mut identify = false;
        page_header(ui, "Monitors", &subtitle, |ui| {
            if widgets::ghost_button(ui, &Icon::MONITOR.with("Identify"))
                .on_hover_text("Show each monitor's number on its screen for a few seconds")
                .clicked()
            {
                identify = true;
            }
        });
        if identify {
            self.tools.identify_start();
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.status_strip(ui);
                ui.add_space(4.0);
                let core = &mut self.core;
                widgets::card_row(ui, &[1.25, 1.0], &mut |ui, i| {
                    if i == 0 {
                        link_all_card(ui, core);
                    } else {
                        night_card(ui, core);
                    }
                });
                ui.add_space(GAP_Y);
                if self.core.live.ready && self.core.live.monitors.is_empty() {
                    widgets::card(ui, |ui| {
                        let p = pal(ui);
                        widgets::empty_state(
                            ui,
                            Icon::MONITOR,
                            p.muted,
                            "No monitor answered",
                            "Turn DDC/CI on in the monitor's own menu (often under Other or System), check the cable, then look again with the refresh button at the top.",
                        );
                    });
                }
                let monitors = self.core.live.monitors.clone();
                let cols = if ui.available_width() >= 820.0 { 2 } else { 1 };
                for row in monitors.chunks(cols) {
                    let weights = vec![1.0; cols];
                    let core = &mut self.core;
                    widgets::card_row(ui, &weights, &mut |ui, i| {
                        if let Some(m) = row.get(i) {
                            monitor_card(ui, core, m);
                        }
                    });
                    ui.add_space(GAP_Y);
                }
            });
    }

    fn status_strip(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let live = self.core.live.clone();
        let shared = self.core.shared.clone();
        let tz = shared.tz.as_ref();
        let mut go_schedule = false;
        let mut go_profiles = false;
        Frame::new()
            .fill(p.card)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    // Schedule.
                    let s = &live.schedule;
                    let (color, text) = if !s.enabled {
                        (p.faint, "Schedule off".to_string())
                    } else if s.following {
                        let since = s
                            .current
                            .as_deref()
                            .map(|c| format!(": {}", lower_first(c)))
                            .unwrap_or_default();
                        (p.good, format!("Following the schedule{since}"))
                    } else {
                        (p.warn, "Schedule paused after a manual change".to_string())
                    };
                    status_item(ui, Icon::CLOCK, color, &text);
                    if s.enabled && !s.following {
                        if widgets::small_button(ui, &Icon::PLAY.with("Resume")).clicked() {
                            self.core.send(Cmd::ResumeSchedule);
                            self.core.toast("Following the schedule again", false);
                        }
                    } else if s.enabled {
                        if let Some((t, _)) = &s.next {
                            ui.label(
                                RichText::new(format!("next {}", crate::timeutil::fmt_hm(*t, tz)))
                                    .size(13.0)
                                    .color(p.muted),
                            );
                        }
                    } else if widgets::small_button(ui, "Set up").clicked() {
                        go_schedule = true;
                    }
                    separator(ui);
                    // App profile.
                    match &live.active_profile {
                        Some(app) => {
                            status_item(ui, Icon::APPS, p.info, &format!("Profile: {app}"));
                        }
                        None => {
                            let n = self
                                .core
                                .settings
                                .profiles
                                .iter()
                                .filter(|p| p.enabled)
                                .count();
                            let text = match n {
                                0 => "No app profiles".to_string(),
                                1 => "1 app profile, not active".to_string(),
                                n => format!("{n} app profiles, none active"),
                            };
                            if status_item(ui, Icon::APPS, p.faint, &text)
                                .on_hover_text("Open App profiles")
                                .clicked()
                            {
                                go_profiles = true;
                            }
                        }
                    }
                    separator(ui);
                    // Idle dimming.
                    let idle = &self.core.settings.idle;
                    let (color, text) = if live.idle_dimmed {
                        (p.warn, "Dimmed while you are away".to_string())
                    } else if idle.enabled {
                        (
                            p.muted,
                            format!(
                                "Dims to {} % after {} min away",
                                idle.brightness, idle.after_minutes
                            ),
                        )
                    } else {
                        (p.faint, "No dimming when away".to_string())
                    };
                    if status_item(ui, Icon::DIM, color, &text)
                        .on_hover_text("Set in App profiles")
                        .clicked()
                    {
                        go_profiles = true;
                    }
                });
            });
        if go_schedule {
            self.tab = Tab::Schedule;
        }
        if go_profiles {
            self.tab = Tab::Profiles;
        }
    }
}

const GAP_Y: f32 = 12.0;

fn status_item(ui: &mut Ui, icon: Icon, color: Color32, text: &str) -> egui::Response {
    let p = pal(ui);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), regular(13.5), p.text);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 24.0, 26.0), Sense::click());
    ui.painter().text(
        rect.left_center() + vec2(8.0, 0.0),
        Align2::CENTER_CENTER,
        icon.glyph(),
        regular(14.0),
        color,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 24.0, rect.center().y - galley.size().y / 2.0),
        galley,
        p.text,
    );
    resp
}

fn separator(ui: &mut Ui) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 20.0), Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0_f32, p.border),
    );
}

fn link_all_card(ui: &mut Ui, core: &mut Core) {
    let p = pal(ui);
    let linked = core
        .live
        .monitors
        .iter()
        .filter(|m| core.settings.prefs(&m.monitor.key).linked)
        .count();
    let total = core.live.monitors.len();
    widgets::split_row(
        ui,
        26.0,
        |ui| widgets::section_icon(ui, Icon::LINK, "All monitors together"),
        |ui| {
            ui.label(
                RichText::new(format!("{linked} of {total} linked"))
                    .size(12.5)
                    .color(p.muted),
            )
            .on_hover_text("Choose which monitors move together under each monitor's settings");
        },
    );
    let min = core.settings.min_brightness();
    let mut v = core.link_value();
    let r = widgets::slider_with_value(
        ui,
        &mut v,
        min..=100,
        1,
        Track::Brightness,
        linked > 0,
        widgets::percent_text,
    );
    if r.changed() {
        core.set_brightness(None, v);
    }
    ui.add_space(2.0);
    let presets: Vec<(String, String)> = core
        .settings
        .presets
        .iter()
        .map(|pr| {
            let mut tip = format!("Brightness {} %", pr.brightness);
            if let Some(c) = pr.contrast {
                tip.push_str(&format!(", contrast {c} %"));
            }
            match pr.kelvin {
                Some(k) => tip.push_str(&format!(", night light {k} K")),
                None => tip.push_str(", night light off"),
            }
            (pr.name.clone(), tip)
        })
        .collect();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (name, tip) in presets {
            let active = core.live.last_preset.as_deref() == Some(name.as_str());
            if widgets::chip(ui, &name, active)
                .on_hover_text(tip)
                .clicked()
            {
                core.apply_preset(&name);
            }
        }
    });
}

fn night_card(ui: &mut Ui, core: &mut Core) {
    let p = pal(ui);
    let on = core.night_on();
    widgets::split_row(
        ui,
        26.0,
        |ui| widgets::section_icon(ui, Icon::MOON, "Night light"),
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
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(36.0, 22.0), Sense::hover());
        let color = if on {
            kelvin_color(k as u32)
        } else {
            kelvin_color(KELVIN_NEUTRAL)
        };
        ui.painter().rect_filled(rect, CornerRadius::same(6), color);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.0_f32, p.border),
            egui::StrokeKind::Inside,
        );
        let text = if on {
            "Warmer colours on every screen. 6500 K is neutral."
        } else {
            "Off: colours are neutral (6500 K)."
        };
        ui.label(RichText::new(text).size(12.5).color(p.muted));
    });
}

/// A label column and a control, on one line.
fn feature_row(ui: &mut Ui, icon: Icon, label: &str, add: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(LABEL_W, 26.0), Sense::hover());
        ui.painter().text(
            rect.left_center() + vec2(8.0, 0.0),
            Align2::CENTER_CENTER,
            icon.glyph(),
            regular(14.0),
            p.muted,
        );
        ui.painter().text(
            rect.left_center() + vec2(24.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            regular(14.0),
            p.text,
        );
        add(ui);
    });
}

fn feature_icon(f: Feature) -> Icon {
    match f {
        Feature::Brightness => Icon::BRIGHTNESS,
        Feature::Contrast => Icon::CONTRAST,
        Feature::Volume => Icon::VOLUME,
        Feature::InputSource => Icon::INPUT,
        Feature::PowerMode => Icon::POWER,
        Feature::ColorPreset | Feature::RedGain | Feature::GreenGain | Feature::BlueGain => {
            Icon::COLOR
        }
    }
}

/// The values a non-continuous feature offers, and its name for a code.
fn choices(m: &MonitorView, f: Feature, current: Option<i32>) -> Vec<u16> {
    let mut list: Vec<u16> = m
        .monitor
        .caps
        .as_ref()
        .map(|c| c.values(f.vcp()).to_vec())
        .unwrap_or_default();
    if list.is_empty() && f == Feature::PowerMode {
        list = vec![1, 4, 5];
    }
    if let Some(c) = current {
        let c = c as u16;
        if !list.contains(&c) {
            list.insert(0, c);
        }
    }
    list
}

pub fn choice_name(f: Feature, code: u16) -> String {
    match f {
        Feature::InputSource => input_name(code),
        Feature::ColorPreset => color_preset_name(code),
        Feature::PowerMode => power_name(code).to_string(),
        _ => code.to_string(),
    }
}

/// One feature's control: a slider for continuous ones, a list for the others.
fn feature_control(ui: &mut Ui, core: &mut Core, m: &MonitorView, f: Feature) {
    let label = f.label();
    let key = m.monitor.key.clone();
    let current = core.feature(m, f);
    if f.continuous() {
        let track = match f {
            Feature::RedGain => Track::Channel(Color32::from_rgb(0xff, 0x5a, 0x5a)),
            Feature::GreenGain => Track::Channel(Color32::from_rgb(0x3e, 0xcf, 0x6e)),
            Feature::BlueGain => Track::Channel(Color32::from_rgb(0x4a, 0x8c, 0xff)),
            _ => Track::Plain,
        };
        feature_row(ui, feature_icon(f), label, |ui| {
            let mut v = current.unwrap_or(0);
            let r = widgets::slider_with_value(
                ui,
                &mut v,
                0..=100,
                1,
                track,
                current.is_some(),
                widgets::percent_text,
            );
            if r.changed() {
                core.set_feature(&key, f, v.max(0) as u32);
            }
        });
    } else {
        let list = choices(m, f, current);
        feature_row(ui, feature_icon(f), label, |ui| {
            let shown = current
                .map(|c| choice_name(f, c as u16))
                .unwrap_or_else(|| "Unknown".into());
            let mut pick = None;
            let combo = egui::ComboBox::from_id_salt(("feature", &key, f))
                .width(200.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for code in &list {
                        let selected = current == Some(*code as i32);
                        if ui
                            .selectable_label(selected, choice_name(f, *code))
                            .clicked()
                        {
                            pick = Some(*code);
                        }
                    }
                });
            if f == Feature::PowerMode {
                combo.response.on_hover_text(
                    "Off and standby blank this screen; most monitors wake with their power button or the next signal.",
                );
            }
            if let Some(code) = pick {
                core.set_feature(&key, f, code as u32);
            }
        });
    }
}

fn monitor_card(ui: &mut Ui, core: &mut Core, m: &MonitorView) {
    let p = pal(ui);
    let key = m.monitor.key.clone();
    let conn = m.monitor.connection;
    // Head: icon, name, badges.
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), p.hover);
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            monitor_icon(conn).glyph(),
            regular(18.0),
            p.accent,
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let name = core.monitor_name(m);
            let g = widgets::elided(
                ui,
                &name,
                semibold(16.0),
                p.text,
                (ui.available_width() - 4.0).max(60.0),
            );
            ui.label(g);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                widgets::badge(ui, conn.label(), connection_color(ui, conn))
                    .on_hover_text(connection_hint(conn));
                widgets::badge(ui, &format!("Display {}", m.monitor.number), p.muted)
                    .on_hover_text("The number shown by Identify and used by --monitor");
                if m.monitor.primary {
                    widgets::badge(ui, "Main", p.accent).on_hover_text("The main Windows display");
                }
                if m.fading {
                    widgets::badge(ui, "Fading", p.info);
                }
            });
        });
    });
    ui.add_space(2.0);

    // Brightness.
    let min = core.settings.min_brightness();
    let mut v = core.brightness(m);
    feature_row(ui, Icon::BRIGHTNESS, "Brightness", |ui| {
        let r = widgets::slider_with_value(
            ui,
            &mut v,
            min..=100,
            1,
            Track::Brightness,
            true,
            widgets::percent_text,
        );
        if r.changed() {
            core.set_brightness(Some(&key), v);
        }
        if min < 0 {
            r.on_hover_text("Below 0 the screen stays at its lowest level and DisplayForge darkens it further in software.");
        }
    });
    if v < 0 {
        ui.horizontal(|ui| {
            ui.add_space(LABEL_W + 8.0);
            ui.label(
                RichText::new("Software dimming below the monitor's lowest level")
                    .size(12.0)
                    .color(p.info),
            );
        });
    } else if conn == Connection::SoftwareOnly {
        ui.horizontal(|ui| {
            ui.add_space(LABEL_W + 8.0);
            ui.label(
                RichText::new("Dimmed in software")
                    .size(12.0)
                    .color(p.muted),
            );
        });
    }

    // The features chosen for the page, then the rest under "More".
    let shown: Vec<Feature> = core
        .settings
        .show_features
        .iter()
        .copied()
        .filter(|f| *f != Feature::Brightness && m.monitor.supports(*f))
        .collect();
    for f in &shown {
        feature_control(ui, core, m, *f);
    }
    let more: Vec<Feature> = Feature::ALL
        .into_iter()
        .filter(|f| *f != Feature::Brightness && m.monitor.supports(*f) && !shown.contains(f))
        .collect();

    // A plain explanation when something is not available.
    let message = m.error.as_ref().map(|e| (e, true)).or(m
        .monitor
        .note
        .as_ref()
        .map(|n| (n, false)));
    if let Some((text, error)) = message {
        let color = if error { p.danger } else { p.warn };
        Frame::new()
            .fill(color.gamma_multiply(0.08))
            .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.35)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.label(
                        RichText::new(if error {
                            Icon::ERROR.glyph()
                        } else {
                            Icon::INFO.glyph()
                        })
                        .color(color),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(text.as_str()).size(13.0).color(p.text))
                            .wrap(),
                    );
                });
            });
    }

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        if !more.is_empty() {
            expander_toggle(ui, ("more", &key), &format!("More ({})", more.len()));
        }
        expander_toggle(ui, ("prefs", &key), "Monitor settings");
    });
    if expander_open(ui, ("more", &key)) && !more.is_empty() {
        for f in &more {
            feature_control(ui, core, m, *f);
        }
    }
    if expander_open(ui, ("prefs", &key)) {
        monitor_prefs(ui, core, m);
    }
}

/// A small text button that opens and closes a section. The state lives in
/// egui's memory so it survives page changes.
fn expander_toggle(ui: &mut Ui, id: impl std::hash::Hash, label: &str) {
    let p = pal(ui);
    let id = egui::Id::new(id);
    let open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let icon = if open {
        Icon::CHEVRON_DOWN
    } else {
        Icon::CHEVRON_RIGHT
    };
    let text = RichText::new(icon.with(label)).size(13.0).color(p.muted);
    if ui.add(egui::Button::new(text).frame(false)).clicked() {
        ui.data_mut(|d| d.insert_temp(id, !open));
    }
}

fn expander_open(ui: &Ui, id: impl std::hash::Hash) -> bool {
    ui.data(|d| d.get_temp::<bool>(egui::Id::new(id)))
        .unwrap_or(false)
}

fn monitor_prefs(ui: &mut Ui, core: &mut Core, m: &MonitorView) {
    let p = pal(ui);
    let key = m.monitor.key.clone();
    Frame::new()
        .fill(p.hover.gamma_multiply(0.6))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            let prefs = core.settings.prefs_mut(&key);
            feature_row(ui, Icon::EDIT, "Name", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut prefs.rename)
                        .hint_text(RichText::new(m.monitor.name.as_str()).color(p.faint))
                        .margin(Margin::symmetric(8, 5))
                        .desired_width(ui.available_width().min(260.0)),
                );
            });
            feature_row(ui, Icon::BRIGHTNESS, "Calibration", |ui| {
                ui.label(RichText::new("0 % is").size(13.0).color(p.muted));
                ui.add(
                    egui::DragValue::new(&mut prefs.min)
                        .range(0..=100)
                        .suffix(" %"),
                );
                ui.label(RichText::new("100 % is").size(13.0).color(p.muted));
                ui.add(
                    egui::DragValue::new(&mut prefs.max)
                        .range(0..=100)
                        .suffix(" %"),
                )
                .on_hover_text("The monitor's own levels that DisplayForge's 0 and 100 map to, so that different monitors look alike at the same number.");
            });
            feature_row(ui, Icon::DIM, "Dim with", |ui| {
                let current = match prefs.dim_method {
                    DimMethod::Gamma => 0,
                    DimMethod::Overlay => 1,
                };
                if let Some(i) = widgets::segmented(ui, &["Gamma", "Overlay"], current) {
                    prefs.dim_method = if i == 0 {
                        DimMethod::Gamma
                    } else {
                        DimMethod::Overlay
                    };
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(LABEL_W + 8.0);
                widgets::hint(
                    ui,
                    "How dimming below the monitor's lowest level is done. Gamma is invisible in screenshots; the overlay works everywhere.",
                );
            });
            ui.horizontal(|ui| {
                ui.add_space(LABEL_W + 8.0);
                ui.spacing_mut().item_spacing.x = 18.0;
                widgets::checkbox(ui, &mut prefs.linked, "Moves with “all monitors”");
                let mut shown = !prefs.hidden;
                if widgets::checkbox(ui, &mut shown, "In the quick panel").changed() {
                    prefs.hidden = !shown;
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_text_helpers() {
        assert_eq!(
            lower_first("Since 16:21 (sunset - 30 min)"),
            "since 16:21 (sunset - 30 min)"
        );
        assert_eq!(lower_first(""), "");
        assert_eq!(choice_name(Feature::InputSource, 0x0F), "DisplayPort 1");
        assert_eq!(choice_name(Feature::PowerMode, 1), "On");
    }

    #[test]
    fn choices_include_the_current_value() {
        let live = crate::demo::live_state();
        let main = &live.monitors[1];
        let inputs = choices(main, Feature::InputSource, Some(0x0F));
        assert_eq!(inputs, vec![0x0F, 0x11, 0x12, 0x1B]);
        let odd = choices(main, Feature::InputSource, Some(0x03));
        assert_eq!(odd[0], 0x03);
        let side = &live.monitors[2];
        assert_eq!(choices(side, Feature::PowerMode, None), vec![0x01, 0x05]);
    }
}
