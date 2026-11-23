//! The Schedule page: on/off, location (sunrise and sunset), the daily
//! points and a 24-hour chart of brightness and colour temperature.

use super::app::{page_header, App};
use super::core::Core;
use super::monitors::lower_first;
use super::theme::{kelvin_color, pal, regular, semibold, Icon};
use super::widgets;
use crate::engine::Cmd;
use crate::settings::{Location, SchedulePoint, When, KELVIN_MIN, KELVIN_NEUTRAL};
use crate::timeutil;
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, Mesh, Rect, RichText, Sense, Shape, Stroke, Ui,
};

#[derive(Default)]
pub struct SchedulePage {
    query: String,
}

/// Brightness and colour temperature at one minute of the day.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub minute: u32,
    pub brightness: Option<i32>,
    pub kelvin: Option<u32>,
}

/// Sunrise and sunset today as local minutes, when known.
pub fn sun_minutes(core: &Core) -> (Option<u32>, Option<u32>) {
    let tz = core.tz();
    let now = core.now();
    let day = timeutil::local_day(now, tz);
    let mut rise = None;
    let mut set = None;
    if let Some(loc) = &core.settings.location {
        let sun = crate::solar::sun_times(day, loc.lat, loc.lon, tz);
        rise = sun.sunrise;
        set = sun.sunset;
    }
    // The engine's figures when the local calculation has none.
    let rise = rise.or(core.live.schedule.sunrise);
    let set = set.or(core.live.schedule.sunset);
    (
        rise.map(|t| timeutil::local_minutes(t, tz)),
        set.map(|t| timeutil::local_minutes(t, tz)),
    )
}

/// The minute of the day a point takes effect, given today's sun times.
pub fn point_minute(when: &When, rise: Option<u32>, set: Option<u32>) -> Option<u32> {
    let wrap = |m: i32| m.rem_euclid(1440) as u32;
    match *when {
        When::Time { minutes } => Some(minutes.min(1439)),
        When::Sunrise { offset } => rise.map(|r| wrap(r as i32 + offset)),
        When::Sunset { offset } => set.map(|s| wrap(s as i32 + offset)),
    }
}

/// The values a point sets (a preset's win).
fn point_values(
    p: &SchedulePoint,
    presets: &[crate::settings::Preset],
) -> (Option<i32>, Option<u32>) {
    if let Some(name) = &p.preset {
        if let Some(pr) = presets.iter().find(|x| x.name.eq_ignore_ascii_case(name)) {
            return (
                Some(pr.brightness),
                Some(pr.kelvin.unwrap_or(KELVIN_NEUTRAL)),
            );
        }
    }
    (p.brightness, p.kelvin)
}

/// A simple day curve from points given as (minute, brightness, kelvin):
/// each point holds until the next, reached gradually over `transition`
/// minutes. Values a point leaves alone carry over from the one before.
pub fn simple_curve(
    points: &[(u32, Option<i32>, Option<u32>)],
    transition: u32,
    step: u32,
) -> Vec<Sample> {
    let mut pts: Vec<(u32, Option<i32>, Option<u32>)> = points.to_vec();
    pts.sort_by_key(|p| p.0);
    if pts.is_empty() {
        return Vec::new();
    }
    // Carry values forward around the day (twice, so the wrap fills in too).
    let n = pts.len();
    let mut eff = pts.clone();
    for i in 0..2 * n {
        let (cur, prev) = (i % n, (i + n - 1) % n);
        if pts[cur].1.is_none() {
            eff[cur].1 = eff[prev].1;
        }
        if pts[cur].2.is_none() {
            eff[cur].2 = eff[prev].2;
        }
    }
    let at = |minute: u32| -> Sample {
        // The last point at or before `minute` (else yesterday's last one).
        let idx = eff.iter().rposition(|p| p.0 <= minute).unwrap_or(n - 1);
        let prev = (idx + n - 1) % n;
        let (t, b, k) = eff[idx];
        let since = (minute as i32 - t as i32).rem_euclid(1440) as u32;
        if transition > 0 && since < transition && n > 1 {
            let f = since as f32 / transition as f32;
            let (_, pb, pk) = eff[prev];
            let mix_b = match (pb, b) {
                (Some(a), Some(z)) => Some((a as f32 + (z - a) as f32 * f).round() as i32),
                _ => b,
            };
            let mix_k = match (pk, k) {
                (Some(a), Some(z)) => {
                    Some((a as f32 + (z as f32 - a as f32) * f).round().max(0.0) as u32)
                }
                _ => k,
            };
            Sample {
                minute,
                brightness: mix_b,
                kelvin: mix_k,
            }
        } else {
            Sample {
                minute,
                brightness: b,
                kelvin: k,
            }
        }
    };
    (0..=1440)
        .step_by(step.max(1) as usize)
        .map(|m| at(m.min(1439)))
        .collect()
}

/// Today's curve: from the schedule logic when it answers, else computed here.
pub fn day_curve(core: &Core) -> Vec<Sample> {
    let tz = core.tz();
    let now = core.now();
    let day = timeutil::local_day(now, tz);
    let mut schedule = core.settings.schedule.clone();
    schedule.enabled = true;
    let loc = core.settings.location.as_ref();
    let step = 10;
    let samples: Vec<Sample> = (0..=1440u32)
        .step_by(step)
        .map(|m| {
            let t = timeutil::local_to_utc(day, m.min(1439), tz);
            let r = crate::schedule::resolve(&schedule, &core.settings.presets, loc, t, tz);
            Sample {
                minute: m,
                brightness: r.as_ref().and_then(|r| r.brightness),
                kelvin: r.as_ref().and_then(|r| r.kelvin),
            }
        })
        .collect();
    if samples
        .iter()
        .any(|s| s.brightness.is_some() || s.kelvin.is_some())
    {
        return samples;
    }
    let (rise, set) = sun_minutes(core);
    let points: Vec<(u32, Option<i32>, Option<u32>)> = schedule
        .points
        .iter()
        .filter_map(|p| {
            let m = point_minute(&p.when, rise, set)?;
            let (b, k) = point_values(p, &core.settings.presets);
            Some((m, b, k))
        })
        .collect();
    let mut out = simple_curve(&points, schedule.transition_minutes, step as u32);
    if let Some(last) = out.last_mut() {
        last.minute = 1440;
    }
    out
}

/// "Sunset - 30 min", "Sunrise", "22:30".
pub fn when_text(when: &When) -> String {
    let label = crate::schedule::when_label(when);
    if !label.is_empty() {
        return label;
    }
    let offset = |base: &str, o: i32| match o {
        0 => base.to_string(),
        o if o > 0 => format!("{base} + {o} min"),
        o => format!("{base} - {} min", -o),
    };
    match *when {
        When::Time { minutes } => timeutil::fmt_minutes(minutes),
        When::Sunrise { offset: o } => offset("Sunrise", o),
        When::Sunset { offset: o } => offset("Sunset", o),
    }
}

/// Parses "7:45" or "07:45" into minutes after midnight.
pub fn parse_hm(text: &str) -> Option<u32> {
    let (h, m) = text.trim().split_once(':')?;
    let h: u32 = h.trim().parse().ok()?;
    let m: u32 = m.trim().parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

impl App {
    pub(super) fn schedule_ui(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let enabled = self.core.settings.schedule.enabled;
        page_header(
            ui,
            "Schedule",
            "Brightness and night light that follow the day",
            |ui| {
                if widgets::toggle(ui, enabled, true).clicked() {
                    self.core.settings.schedule.enabled = !enabled;
                }
                ui.label(
                    RichText::new(if enabled {
                        "Schedule on"
                    } else {
                        "Schedule off"
                    })
                    .color(if enabled { p.text } else { p.muted }),
                );
            },
        );
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = widgets::GAP;
                    let right_w = 360.0;
                    let left_w = ui.available_width() - right_w - widgets::GAP;
                    ui.allocate_ui_with_layout(
                        vec2(left_w, 10.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = widgets::GAP;
                            widgets::card(ui, |ui| chart_card(ui, &self.core));
                            let core = &mut self.core;
                            widgets::card(ui, |ui| points_card(ui, core));
                        },
                    );
                    ui.allocate_ui_with_layout(
                        vec2(right_w, 10.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = widgets::GAP;
                            let core = &mut self.core;
                            widgets::card(ui, |ui| now_card(ui, core));
                            let query = &mut self.schedule.query;
                            widgets::card(ui, |ui| location_card(ui, core, query));
                        },
                    );
                });
            });
    }
}

fn now_card(ui: &mut Ui, core: &mut Core) {
    let p = pal(ui);
    widgets::section_icon(ui, Icon::CLOCK, "Right now");
    let s = core.live.schedule.clone();
    let tz = core.tz();
    if !core.settings.schedule.enabled {
        widgets::hint(
            ui,
            "The schedule is off: brightness and night light stay as you set them. Switch it on at the top right.",
        );
    } else if s.following {
        let text = match &s.current {
            Some(c) => format!("Following the schedule: {}", lower_first(c)),
            None => "Following the schedule".to_string(),
        };
        ui.label(RichText::new(text).color(p.good));
        if let Some((t, label)) = &s.next {
            ui.label(
                RichText::new({
                    let at = timeutil::fmt_hm(*t, tz);
                    if *label == at || label.is_empty() {
                        format!("Next change at {at}")
                    } else {
                        format!("Next change at {at} ({label})")
                    }
                })
                .size(13.0)
                .color(p.muted),
            );
        }
    } else {
        ui.label(RichText::new("Paused after a manual change").color(p.warn));
        widgets::hint(
            ui,
            "A manual change pauses the schedule until its next point. Resume to apply the scheduled values now.",
        );
        if widgets::small_button(ui, &Icon::PLAY.with("Resume")).clicked() {
            core.send(Cmd::ResumeSchedule);
            core.toast("Following the schedule again", false);
        }
    }
    ui.add_space(4.0);
    widgets::setting_row_w(
        ui,
        "Gradual change",
        "Move from one point's values to the next over this long.",
        110.0,
        |ui| {
            ui.add(
                egui::DragValue::new(&mut core.settings.schedule.transition_minutes)
                    .range(0..=180)
                    .suffix(" min"),
            );
        },
    );
}

fn location_card(ui: &mut Ui, core: &mut Core, query: &mut String) {
    let p = pal(ui);
    widgets::section_icon(ui, Icon::MAP_PIN, "Location");
    match core.settings.location.clone() {
        Some(loc) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&loc.name).font(semibold(14.5)));
                ui.label(
                    RichText::new(format!("{:.2}, {:.2}", loc.lat, loc.lon))
                        .size(12.5)
                        .color(p.muted),
                );
            });
            let (rise, set) = sun_minutes(core);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(Icon::BRIGHTNESS.glyph()).color(p.accent));
                ui.label(format!(
                    "Sunrise {}",
                    rise.map(timeutil::fmt_minutes)
                        .unwrap_or_else(|| "–".into())
                ));
                ui.add_space(10.0);
                ui.label(RichText::new(Icon::MOON.glyph()).color(p.info));
                ui.label(format!(
                    "Sunset {}",
                    set.map(timeutil::fmt_minutes).unwrap_or_else(|| "–".into())
                ));
            });
            if rise.is_none() && set.is_none() {
                widgets::hint(
                    ui,
                    "The sun does not rise or set here today; sunrise and sunset points are skipped.",
                );
            }
        }
        None => {
            widgets::hint(
                ui,
                "Choose a city or type coordinates to use sunrise and sunset. Worked out on this PC; nothing is looked up online.",
            );
        }
    }
    ui.add(
        egui::TextEdit::singleline(query)
            .hint_text(RichText::new("Search a city, or type 36.19, 44.01").color(p.faint))
            .margin(egui::Margin::symmetric(8, 6))
            .desired_width(f32::INFINITY),
    );
    let q = query.trim().to_string();
    if q.is_empty() {
        return;
    }
    let mut pick: Option<Location> = None;
    if let Some((lat, lon)) = crate::cities::parse_coords(&q) {
        if widgets::small_button(ui, &format!("Use {lat:.3}, {lon:.3}")).clicked() {
            pick = Some(Location {
                name: format!("{lat:.3}, {lon:.3}"),
                lat,
                lon,
            });
        }
    }
    let results = crate::cities::search(&q);
    for city in results.iter().take(6) {
        let label = format!("{}, {}", city.name, city.country);
        if ui
            .add(egui::Button::new(RichText::new(&label)).frame(false))
            .clicked()
        {
            pick = Some(Location {
                name: label,
                lat: city.lat,
                lon: city.lon,
            });
        }
    }
    if results.is_empty() && crate::cities::parse_coords(&q).is_none() {
        ui.label(
            RichText::new("No city by that name in the built-in list. Coordinates work too.")
                .size(12.5)
                .color(p.muted),
        );
    }
    if let Some(loc) = pick {
        core.toast(format!("Location set to {}", loc.name), false);
        core.settings.location = Some(loc);
        query.clear();
    }
}

fn points_card(ui: &mut Ui, core: &mut Core) {
    let p = pal(ui);
    let mut add = false;
    widgets::split_row(
        ui,
        30.0,
        |ui| widgets::section_icon(ui, Icon::LIST, "Points through the day"),
        |ui| {
            if widgets::small_button(ui, &Icon::ADD.with("Add a point")).clicked() {
                add = true;
            }
        },
    );
    widgets::hint(
        ui,
        "From each point on, its values apply until the next one. Sunrise and sunset move with the seasons.",
    );
    let (rise, set) = sun_minutes(core);
    let preset_names: Vec<String> = core
        .settings
        .presets
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let mut remove = None;
    let count = core.settings.schedule.points.len();
    if count == 0 {
        ui.label(RichText::new("No points yet.").color(p.muted));
    }
    for (i, point) in core.settings.schedule.points.iter_mut().enumerate() {
        widgets::divider(ui);
        ui.push_id(("point", i), |ui| {
            // Line 1: when.
            ui.horizontal(|ui| {
                let kind = match point.when {
                    When::Time { .. } => 0,
                    When::Sunrise { .. } => 1,
                    When::Sunset { .. } => 2,
                };
                if let Some(k) = widgets::segmented(ui, &["Time", "Sunrise", "Sunset"], kind) {
                    point.when = match k {
                        0 => When::Time {
                            minutes: point_minute(&point.when, rise, set).unwrap_or(8 * 60),
                        },
                        1 => When::Sunrise { offset: 0 },
                        _ => When::Sunset { offset: 0 },
                    };
                }
                match &mut point.when {
                    When::Time { minutes } => {
                        ui.add(
                            egui::DragValue::new(minutes)
                                .range(0..=1439)
                                .speed(5.0)
                                .custom_formatter(|v, _| timeutil::fmt_minutes(v as u32))
                                .custom_parser(|s| parse_hm(s).map(|m| m as f64)),
                        )
                        .on_hover_text("Drag or type a time such as 22:30");
                    }
                    When::Sunrise { offset } | When::Sunset { offset } => {
                        ui.add(
                            egui::DragValue::new(offset)
                                .range(-240..=240)
                                .speed(1.0)
                                .custom_formatter(|v, _| {
                                    if v >= 0.0 {
                                        format!("+ {v:.0} min")
                                    } else {
                                        format!("- {:.0} min", -v)
                                    }
                                })
                                .custom_parser(|s| {
                                    let t: String =
                                        s.chars().filter(|c| !c.is_whitespace()).collect();
                                    t.trim_end_matches("min").parse::<f64>().ok()
                                }),
                        )
                        .on_hover_text("Minutes before (-) or after (+)");
                    }
                }
                let described = when_text(&point.when);
                if let Some(m) = point_minute(&point.when, rise, set) {
                    if !matches!(point.when, When::Time { .. }) {
                        ui.label(
                            RichText::new(format!("today {}", timeutil::fmt_minutes(m)))
                                .size(12.5)
                                .color(p.muted),
                        )
                        .on_hover_text(described);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::DELETE, "Remove this point").clicked() {
                        remove = Some(i);
                    }
                });
            });
            // Line 2: what.
            ui.horizontal(|ui| {
                let shown = point
                    .preset
                    .clone()
                    .unwrap_or_else(|| "Own values".to_string());
                egui::ComboBox::from_id_salt("what")
                    .width(130.0)
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(point.preset.is_none(), "Own values")
                            .clicked()
                        {
                            point.preset = None;
                        }
                        for name in &preset_names {
                            if ui
                                .selectable_label(point.preset.as_ref() == Some(name), name)
                                .clicked()
                            {
                                point.preset = Some(name.clone());
                            }
                        }
                    });
                if point.preset.is_none() {
                    let mut has_b = point.brightness.is_some();
                    if widgets::checkbox(ui, &mut has_b, "Brightness").changed() {
                        point.brightness = has_b.then_some(60);
                    }
                    if let Some(b) = &mut point.brightness {
                        ui.add(egui::DragValue::new(b).range(-50..=100).suffix(" %"));
                    }
                    ui.add_space(6.0);
                    let mut has_k = point.kelvin.is_some();
                    if widgets::checkbox(ui, &mut has_k, "Colour").changed() {
                        point.kelvin = has_k.then_some(4500);
                    }
                    if let Some(k) = &mut point.kelvin {
                        ui.add(
                            egui::DragValue::new(k)
                                .range(KELVIN_MIN..=KELVIN_NEUTRAL)
                                .speed(25.0)
                                .suffix(" K"),
                        );
                        let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        ui.painter()
                            .circle_filled(r.center(), 6.0, kelvin_color(*k));
                    }
                } else {
                    ui.label(
                        RichText::new("The preset's brightness and night light")
                            .size(12.5)
                            .color(p.muted),
                    );
                }
            });
        });
    }
    if let Some(i) = remove {
        core.settings.schedule.points.remove(i);
    }
    if add {
        core.settings.schedule.points.push(SchedulePoint::default());
    }
}

fn chart_card(ui: &mut Ui, core: &Core) {
    let p = pal(ui);
    widgets::split_row(
        ui,
        24.0,
        |ui| widgets::section(ui, "Today"),
        |ui| {
            legend(ui, p.info, "Colour temperature");
            ui.add_space(8.0);
            legend(ui, p.accent, "Brightness");
        },
    );
    let curve = day_curve(core);
    let (rise, set) = sun_minutes(core);
    let now = timeutil::local_minutes(core.now(), core.tz());
    let points: Vec<u32> = core
        .settings
        .schedule
        .points
        .iter()
        .filter_map(|pt| point_minute(&pt.when, rise, set))
        .collect();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 230.0), Sense::hover());
    paint_chart(ui, rect, &curve, rise, set, now, &points);
}

fn legend(ui: &mut Ui, color: Color32, text: &str) {
    let p = pal(ui);
    ui.label(RichText::new(text).size(12.5).color(p.muted));
    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    ui.painter().line_segment(
        [r.left_center(), r.right_center()],
        Stroke::new(2.5_f32, color),
    );
}

pub fn paint_chart(
    ui: &Ui,
    rect: Rect,
    curve: &[Sample],
    rise: Option<u32>,
    set: Option<u32>,
    now: u32,
    points: &[u32],
) {
    let p = pal(ui);
    let painter = ui.painter_at(rect.expand(2.0));
    let plot = Rect::from_min_max(
        pos2(rect.left() + 40.0, rect.top() + 22.0),
        pos2(rect.right() - 52.0, rect.bottom() - 38.0),
    );
    let x_of = |m: u32| egui::lerp(plot.left()..=plot.right(), m as f32 / 1440.0);
    let yb = |b: i32| {
        egui::lerp(
            plot.bottom()..=plot.top(),
            ((b + 50) as f32 / 150.0).clamp(0.0, 1.0),
        )
    };
    let yk = |k: u32| {
        egui::lerp(
            plot.bottom()..=plot.top(),
            ((k as f32 - KELVIN_MIN as f32) / (KELVIN_NEUTRAL - KELVIN_MIN) as f32).clamp(0.0, 1.0),
        )
    };
    // Night: before sunrise and after sunset.
    let night = p.hover.gamma_multiply(if p.dark { 0.9 } else { 1.0 });
    if let (Some(r), Some(s)) = (rise, set) {
        if r < s {
            painter.rect_filled(
                Rect::from_min_max(plot.min, pos2(x_of(r), plot.bottom())),
                0,
                night,
            );
            painter.rect_filled(
                Rect::from_min_max(pos2(x_of(s), plot.top()), plot.max),
                0,
                night,
            );
        }
    }
    // Grid and axes.
    for b in [0, 50, 100] {
        let y = yb(b);
        painter.hline(plot.x_range(), y, Stroke::new(1.0_f32, p.border));
        painter.text(
            pos2(plot.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            format!("{b} %"),
            regular(11.0),
            p.muted,
        );
    }
    let zero = yb(0);
    painter.text(
        pos2(plot.left() - 6.0, yb(-50)),
        Align2::RIGHT_CENTER,
        "-50",
        regular(11.0),
        p.faint,
    );
    for k in [KELVIN_MIN, 4200, KELVIN_NEUTRAL] {
        painter.text(
            pos2(plot.right() + 6.0, yk(k)),
            Align2::LEFT_CENTER,
            format!("{k} K"),
            regular(11.0),
            p.muted,
        );
    }
    for h in (0..=24).step_by(3) {
        let x = x_of(h * 60);
        painter.vline(
            x,
            plot.bottom()..=plot.bottom() + 4.0,
            Stroke::new(1.0_f32, p.border),
        );
        painter.text(
            pos2(x, plot.bottom() + 6.0),
            Align2::CENTER_TOP,
            format!("{:02}", h % 24),
            regular(11.0),
            p.muted,
        );
    }
    // Colour band under the chart.
    let band = Rect::from_min_max(
        pos2(plot.left(), plot.bottom() + 24.0),
        pos2(plot.right(), plot.bottom() + 34.0),
    );
    if !curve.is_empty() {
        let mut mesh = Mesh::default();
        for (i, s) in curve.iter().enumerate() {
            let c = kelvin_color(s.kelvin.unwrap_or(KELVIN_NEUTRAL));
            let x = x_of(s.minute.min(1440));
            mesh.colored_vertex(pos2(x, band.top()), c);
            mesh.colored_vertex(pos2(x, band.bottom()), c);
            if i > 0 {
                let b = i as u32 * 2;
                mesh.add_triangle(b - 2, b - 1, b);
                mesh.add_triangle(b - 1, b, b + 1);
            }
        }
        painter.add(Shape::mesh(mesh));
    }
    // Brightness: area and line.
    let bpts: Vec<egui::Pos2> = curve
        .iter()
        .filter_map(|s| s.brightness.map(|b| pos2(x_of(s.minute), yb(b))))
        .collect();
    if bpts.len() > 1 {
        let mut mesh = Mesh::default();
        let fill = p.accent.gamma_multiply(0.16);
        for (i, pt) in bpts.iter().enumerate() {
            mesh.colored_vertex(*pt, fill);
            mesh.colored_vertex(pos2(pt.x, zero.max(pt.y)), fill);
            if i > 0 {
                let b = i as u32 * 2;
                mesh.add_triangle(b - 2, b - 1, b);
                mesh.add_triangle(b - 1, b, b + 1);
            }
        }
        painter.add(Shape::mesh(mesh));
        painter.add(Shape::line(bpts, Stroke::new(2.5_f32, p.accent)));
    }
    let kpts: Vec<egui::Pos2> = curve
        .iter()
        .filter_map(|s| s.kelvin.map(|k| pos2(x_of(s.minute), yk(k))))
        .collect();
    if kpts.len() > 1 {
        painter.add(Shape::dashed_line(
            &kpts,
            Stroke::new(2.0_f32, p.info),
            6.0,
            4.0,
        ));
    }
    if curve
        .iter()
        .all(|s| s.brightness.is_none() && s.kelvin.is_none())
    {
        painter.text(
            plot.center(),
            Align2::CENTER_CENTER,
            "Add a point to see the day",
            regular(13.0),
            p.muted,
        );
    }
    // Point markers.
    for m in points {
        if let Some(s) = curve
            .iter()
            .min_by_key(|s| (s.minute as i32 - *m as i32).abs())
        {
            if let Some(b) = s.brightness {
                painter.circle(
                    pos2(x_of(*m), yb(b)),
                    4.0,
                    p.card,
                    Stroke::new(2.0_f32, p.accent),
                );
            }
        }
    }
    // Sunrise and sunset.
    for (m, icon, label) in [
        (rise, Icon::BRIGHTNESS, "Sunrise"),
        (set, Icon::MOON, "Sunset"),
    ] {
        if let Some(m) = m {
            let x = x_of(m);
            painter.vline(
                x,
                plot.y_range(),
                Stroke::new(1.0_f32, p.faint.gamma_multiply(0.6)),
            );
            painter.text(
                pos2(x, rect.top() + 9.0),
                Align2::CENTER_CENTER,
                format!("{} {} {}", icon.glyph(), label, timeutil::fmt_minutes(m))
                    .trim()
                    .to_string(),
                regular(11.5),
                p.muted,
            );
        }
    }
    // Now.
    let x = x_of(now);
    painter.vline(x, plot.y_range(), Stroke::new(1.5_f32, p.text));
    let label = format!("Now {}", timeutil::fmt_minutes(now));
    let g = painter.layout_no_wrap(label, semibold(11.5), p.on_accent);
    let w = g.size().x + 12.0;
    let left = (x - w / 2.0).clamp(plot.left(), plot.right() - w);
    let pill = Rect::from_min_size(pos2(left, plot.top() - 2.0), vec2(w, 18.0));
    painter.rect_filled(pill, CornerRadius::same(9), p.text);
    painter.galley(
        pos2(pill.left() + 6.0, pill.center().y - g.size().y / 2.0),
        g,
        p.bg,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_parse_and_format() {
        assert_eq!(parse_hm("7:45"), Some(465));
        assert_eq!(parse_hm(" 22:30 "), Some(1350));
        assert_eq!(parse_hm("24:00"), None);
        assert_eq!(parse_hm("nope"), None);
        assert_eq!(
            point_minute(&When::Sunset { offset: -30 }, Some(400), Some(1011)),
            Some(981)
        );
        assert_eq!(
            point_minute(&When::Sunrise { offset: -60 }, Some(30), None),
            Some(1410)
        );
        assert_eq!(point_minute(&When::Sunset { offset: 0 }, None, None), None);
        assert_eq!(
            point_minute(&When::Time { minutes: 5000 }, None, None),
            Some(1439)
        );
    }

    #[test]
    fn simple_curve_steps_and_blends() {
        let pts = [(360, Some(80), Some(6500)), (1080, Some(40), None)];
        let c = simple_curve(&pts, 60, 10);
        let at = |m: u32| c.iter().find(|s| s.minute == m).copied().unwrap();
        // Before the first point: yesterday's last values (kelvin carried over).
        assert_eq!(at(120).brightness, Some(40));
        assert_eq!(at(120).kelvin, Some(6500));
        // Half-way through the transition after 06:00.
        assert_eq!(at(390).brightness, Some(60));
        assert_eq!(at(600).brightness, Some(80));
        assert_eq!(at(1200).brightness, Some(40));
        assert!(simple_curve(&[], 30, 10).is_empty());
    }
}
