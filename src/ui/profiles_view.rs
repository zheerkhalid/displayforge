//! The App profiles page: values that apply while an app is in front (in
//! priority order), and dimming while away.

use super::app::{page_header, App};
use super::core::Core;
use super::theme::{kelvin_color, pal, semibold, Icon};
use super::widgets;
use crate::settings::{AppProfile, Trigger, KELVIN_MIN, KELVIN_NEUTRAL};
use eframe::egui::{self, vec2, RichText, Sense, Ui};

#[derive(Default)]
pub struct ProfilesPage {
    new_exe: String,
}

/// "VLC" -> "vlc.exe"; empty or unusable names give `None`.
pub fn normalize_exe(name: &str) -> Option<String> {
    let name = name.trim().trim_matches('"');
    let name = name.rsplit(['\\', '/']).next().unwrap_or(name).trim();
    if name.is_empty() || name.contains(['<', '>', ':', '|', '?', '*']) {
        return None;
    }
    let lower = name.to_lowercase();
    Some(if lower.ends_with(".exe") {
        lower
    } else {
        format!("{lower}.exe")
    })
}

impl App {
    pub(super) fn profiles_ui(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "App profiles",
            "Brightness, contrast and colour while an app is in front; first match wins",
            |_| {},
        );
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = widgets::GAP;
                    let right_w = 340.0;
                    let left_w = ui.available_width() - right_w - widgets::GAP;
                    ui.allocate_ui_with_layout(
                        vec2(left_w, 10.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            let core = &mut self.core;
                            widgets::card(ui, |ui| profiles_card(ui, core));
                        },
                    );
                    ui.allocate_ui_with_layout(
                        vec2(right_w, 10.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = widgets::GAP;
                            let core = &mut self.core;
                            let new_exe = &mut self.profiles.new_exe;
                            widgets::card(ui, |ui| add_card(ui, core, new_exe));
                            widgets::card(ui, |ui| idle_card(ui, core));
                        },
                    );
                });
            });
    }
}

fn profiles_card(ui: &mut Ui, core: &mut Core) {
    let p = pal(ui);
    widgets::section_icon(ui, Icon::APPS, "Profiles");
    let active = core.live.active_profile.clone();
    let n = core.settings.profiles.len();
    if n == 0 {
        widgets::empty_state(
            ui,
            Icon::APPS,
            p.muted,
            "No app profiles yet",
            "Add an app on the right: for example full brightness and no night light while a video player is full screen.",
        );
        return;
    }
    widgets::hint(
        ui,
        "When several apps match, the one higher in the list wins. Values left unticked stay as they are.",
    );
    let mut action: Option<(usize, i32)> = None; // (index, -1 up, +1 down, 0 delete)
    let min = core.settings.min_brightness();
    for (i, pr) in core.settings.profiles.iter_mut().enumerate() {
        widgets::divider(ui);
        let is_active = active
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(&pr.exe));
        ui.push_id(("profile", i), |ui| {
            ui.horizontal(|ui| {
                if widgets::toggle(ui, pr.enabled, true)
                    .on_hover_text("Use this profile")
                    .clicked()
                {
                    pr.enabled = !pr.enabled;
                }
                widgets::badge(ui, &format!("{}", i + 1), p.muted)
                    .on_hover_text("Priority: lower numbers win");
                ui.label(
                    RichText::new(&pr.exe)
                        .font(semibold(15.0))
                        .color(if pr.enabled { p.text } else { p.faint }),
                );
                if is_active {
                    widgets::badge(ui, "Active now", p.good);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::DELETE, "Remove this profile").clicked() {
                        action = Some((i, 0));
                    }
                    if widgets::icon_button_enabled(ui, Icon::DOWN, "Lower priority", i + 1 < n)
                        .clicked()
                    {
                        action = Some((i, 1));
                    }
                    if widgets::icon_button_enabled(ui, Icon::UP, "Higher priority", i > 0)
                        .clicked()
                    {
                        action = Some((i, -1));
                    }
                    let cur = match pr.trigger {
                        Trigger::Foreground => 0,
                        Trigger::Fullscreen => 1,
                    };
                    if let Some(t) = widgets::segmented(ui, &["In front", "Full screen"], cur) {
                        pr.trigger = if t == 0 {
                            Trigger::Foreground
                        } else {
                            Trigger::Fullscreen
                        };
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.add_space(46.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                let mut has = pr.brightness.is_some();
                if widgets::checkbox(ui, &mut has, "Brightness").changed() {
                    pr.brightness = has.then_some(100);
                }
                if let Some(b) = &mut pr.brightness {
                    ui.add(egui::DragValue::new(b).range(min..=100).suffix(" %"));
                }
                ui.add_space(10.0);
                let mut has = pr.contrast.is_some();
                if widgets::checkbox(ui, &mut has, "Contrast").changed() {
                    pr.contrast = has.then_some(70);
                }
                if let Some(c) = &mut pr.contrast {
                    ui.add(egui::DragValue::new(c).range(0..=100).suffix(" %"));
                }
                ui.add_space(10.0);
                let mut has = pr.kelvin.is_some();
                if widgets::checkbox(ui, &mut has, "Colour")
                    .on_hover_text("6500 K pauses night light while the app is in front")
                    .changed()
                {
                    pr.kelvin = has.then_some(KELVIN_NEUTRAL);
                }
                if let Some(k) = &mut pr.kelvin {
                    ui.add(
                        egui::DragValue::new(k)
                            .range(KELVIN_MIN..=KELVIN_NEUTRAL)
                            .speed(25.0)
                            .custom_formatter(|v, _| {
                                if v as u32 >= KELVIN_NEUTRAL {
                                    "6500 K (neutral)".to_string()
                                } else {
                                    format!("{v:.0} K")
                                }
                            }),
                    );
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    ui.painter()
                        .circle_filled(r.center(), 6.0, kelvin_color(*k));
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(46.0);
                widgets::checkbox(
                    ui,
                    &mut pr.only_its_monitor,
                    "Only the monitor the app is on",
                )
                .on_hover_text("Otherwise every monitor changes while the app is in front");
            });
        });
    }
    if let Some((i, dir)) = action {
        let list = &mut core.settings.profiles;
        match dir {
            0 => {
                let removed = list.remove(i);
                core.toast(format!("Removed the profile for {}", removed.exe), false);
            }
            -1 if i > 0 => list.swap(i, i - 1),
            1 if i + 1 < list.len() => list.swap(i, i + 1),
            _ => {}
        }
    }
}

fn add_profile(core: &mut Core, exe: &str) -> bool {
    let Some(exe) = normalize_exe(exe) else {
        core.toast("Type the program's file name, for example vlc.exe", true);
        return false;
    };
    if core
        .settings
        .profiles
        .iter()
        .any(|p| p.exe.eq_ignore_ascii_case(&exe))
    {
        core.toast(format!("{exe} already has a profile"), true);
        return false;
    }
    core.settings.profiles.push(AppProfile {
        exe: exe.clone(),
        ..AppProfile::default()
    });
    core.toast(format!("Added a profile for {exe}"), false);
    true
}

fn add_card(ui: &mut Ui, core: &mut Core, new_exe: &mut String) {
    let p = pal(ui);
    widgets::section_icon(ui, Icon::ADD, "Add a profile");
    let mut add = false;
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(new_exe)
                .hint_text(RichText::new("Program, e.g. vlc.exe").color(pal(ui).faint))
                .margin(egui::Margin::symmetric(8, 6))
                .desired_width(ui.available_width() - 70.0),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            add = true;
        }
        if widgets::small_button(ui, "Add").clicked() {
            add = true;
        }
    });
    if add && add_profile(core, new_exe) {
        new_exe.clear();
    }
    let running: Vec<String> = core
        .live
        .running_apps
        .iter()
        .filter(|a| {
            !core
                .settings
                .profiles
                .iter()
                .any(|p| p.exe.eq_ignore_ascii_case(a))
        })
        .cloned()
        .collect();
    if running.is_empty() {
        return;
    }
    ui.label(RichText::new("Open now").size(12.5).color(p.muted));
    let mut pick = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for app in &running {
            if widgets::chip(ui, &app.to_string(), false)
                .on_hover_text("Add a profile for this app")
                .clicked()
            {
                pick = Some(app.clone());
            }
        }
    });
    if let Some(app) = pick {
        add_profile(core, &app);
    }
}

fn idle_card(ui: &mut Ui, core: &mut Core) {
    let min = core.settings.min_brightness();
    let idle = &mut core.settings.idle;
    widgets::split_row(
        ui,
        26.0,
        |ui| widgets::section_icon(ui, Icon::DIM, "Dim when I'm away"),
        |ui| {
            if widgets::toggle(ui, idle.enabled, true).clicked() {
                idle.enabled = !idle.enabled;
            }
        },
    );
    widgets::hint(
        ui,
        "No keyboard or mouse input for a while dims every screen; the first movement brings it back.",
    );
    let on = idle.enabled;
    ui.add_enabled_ui(on, |ui| {
        widgets::setting_row_w(ui, "After", "", 120.0, |ui| {
            ui.add(
                egui::DragValue::new(&mut idle.after_minutes)
                    .range(1..=240)
                    .suffix(" min"),
            );
        });
        widgets::setting_row_w(
            ui,
            "Dim to",
            "Or the current brightness, if lower.",
            120.0,
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut idle.brightness)
                        .range(min..=100)
                        .suffix(" %"),
                );
            },
        );
        widgets::checkbox(
            ui,
            &mut idle.skip_fullscreen,
            "Not while a full-screen app is in front",
        )
        .on_hover_text("Videos and games often go without input for a long time");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_names_are_normalised() {
        assert_eq!(normalize_exe("VLC").as_deref(), Some("vlc.exe"));
        assert_eq!(normalize_exe(" gimp.EXE ").as_deref(), Some("gimp.exe"));
        assert_eq!(
            normalize_exe("C:\\Program Files\\App\\app.exe").as_deref(),
            Some("app.exe")
        );
        assert_eq!(normalize_exe("   "), None);
        assert_eq!(normalize_exe("a|b"), None);
    }
}
