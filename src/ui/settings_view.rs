//! The Settings page: appearance and Windows integration, brightness
//! behaviour, presets, hotkeys, the settings file and About.

use super::app::{page_header, paint_logo, App};
use super::core::Core;
use super::theme::{kelvin_color, pal, semibold, Icon};
use super::widgets;
use crate::model::Feature;
use crate::settings::{Preset, Settings, ThemeChoice, KELVIN_MIN, KELVIN_NEUTRAL};
use eframe::egui::{self, vec2, RichText, Sense, Ui};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct SettingsPage {
    pub applied_theme: Option<ThemeChoice>,
    startup: Option<bool>,
    /// The hotkey being recorded.
    recording: Option<usize>,
    /// The last problem with a recorded hotkey, by row.
    hotkey_error: Option<(usize, String)>,
}

/// The name `hotkey::parse` expects for a key, or `None` for keys that
/// cannot be a hotkey.
pub fn key_name(key: egui::Key) -> Option<String> {
    use egui::Key;
    let name = match key {
        Key::ArrowUp => "Up".to_string(),
        Key::ArrowDown => "Down".to_string(),
        Key::ArrowLeft => "Left".to_string(),
        Key::ArrowRight => "Right".to_string(),
        Key::PageUp => "PageUp".to_string(),
        Key::PageDown => "PageDown".to_string(),
        Key::Home => "Home".to_string(),
        Key::End => "End".to_string(),
        Key::Insert => "Insert".to_string(),
        Key::Delete => "Delete".to_string(),
        Key::Space => "Space".to_string(),
        Key::Enter => "Enter".to_string(),
        Key::Tab => "Tab".to_string(),
        Key::Backspace => "Backspace".to_string(),
        Key::Minus => "-".to_string(),
        Key::Plus | Key::Equals => "=".to_string(),
        Key::Comma => ",".to_string(),
        Key::Period => ".".to_string(),
        Key::Escape => return None,
        k => {
            let n = k.name();
            let ok = (n.len() == 1 && n.chars().all(|c| c.is_ascii_alphanumeric()))
                || (n.starts_with('F')
                    && n.len() <= 3
                    && n[1..].chars().all(|c| c.is_ascii_digit()));
            if !ok {
                return None;
            }
            n.to_string()
        }
    };
    Some(name)
}

/// "Ctrl+Alt+Up" from modifiers and a key name.
pub fn combo_text(ctrl: bool, alt: bool, shift: bool, key: &str) -> String {
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if alt {
        parts.push("Alt");
    }
    if shift {
        parts.push("Shift");
    }
    parts.push(key);
    parts.join("+")
}

/// Why a recorded hotkey cannot be used, if it cannot.
pub fn hotkey_problem(text: &str, others: &[String]) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let has_mod = lower.contains("ctrl+") || lower.contains("alt+") || lower.contains("win+");
    let last = text.rsplit('+').next().unwrap_or("");
    let function_key =
        last.len() >= 2 && last.starts_with('F') && last[1..].chars().all(|c| c.is_ascii_digit());
    if !has_mod && !function_key {
        return Some("Add Ctrl or Alt, so the key still works normally in other programs".into());
    }
    if others.iter().any(|o| o.eq_ignore_ascii_case(text)) {
        return Some(format!("{text} is already used by another hotkey"));
    }
    if let Err(e) = crate::hotkey::parse(text) {
        return Some(e);
    }
    None
}

impl App {
    pub(super) fn settings_ui(&mut self, ui: &mut Ui) {
        let mut about = false;
        page_header(
            ui,
            "Settings",
            "How DisplayForge starts, fades and responds",
            |ui| {
                if widgets::ghost_button(ui, &Icon::INFO.with("About")).clicked() {
                    about = true;
                }
            },
        );
        if about {
            self.about_open = true;
        }
        if self.settings_page.startup.is_none() {
            self.settings_page.startup =
                Some(!self.core.demo && crate::sys::startup_enabled(crate::sys::RUN_VALUE));
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = widgets::GAP;
                    let w = (ui.available_width() - widgets::GAP) / 2.0;
                    ui.allocate_ui_with_layout(
                        vec2(w, 10.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = widgets::GAP;
                            let core = &mut self.core;
                            let page = &mut self.settings_page;
                            widgets::card(ui, |ui| general_card(ui, core, page));
                            widgets::card(ui, |ui| brightness_card(ui, core));
                            widgets::card(ui, |ui| presets_card(ui, core));
                        },
                    );
                    ui.allocate_ui_with_layout(
                        vec2(w, 10.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = widgets::GAP;
                            let core = &mut self.core;
                            let page = &mut self.settings_page;
                            widgets::card(ui, |ui| hotkeys_card(ui, core, page));
                            widgets::card(ui, |ui| file_card(ui, core));
                            let logo = self.logo.clone();
                            let mut open_about = false;
                            widgets::card(ui, |ui| {
                                open_about = about_card(ui, logo.as_ref());
                            });
                            if open_about {
                                self.about_open = true;
                            }
                        },
                    );
                });
            });
    }
}

fn general_card(ui: &mut Ui, core: &mut Core, page: &mut SettingsPage) {
    widgets::section(ui, "Appearance and Windows");
    let demo = core.demo;
    let s = &mut core.settings;
    widgets::setting_row(ui, "Theme", "", |ui| {
        let current = match s.theme {
            ThemeChoice::System => 0,
            ThemeChoice::Dark => 1,
            ThemeChoice::Light => 2,
        };
        if let Some(i) = widgets::segmented(ui, &["System", "Dark", "Light"], current) {
            s.theme = [ThemeChoice::System, ThemeChoice::Dark, ThemeChoice::Light][i];
        }
    });
    let startup = page.startup.unwrap_or(false);
    let mut toast: Option<(String, bool)> = None;
    widgets::setting_row(
        ui,
        "Start with Windows",
        "Starts quietly in the notification area when you sign in.",
        |ui| {
            let r = widgets::toggle(ui, startup, !demo);
            if demo {
                r.on_hover_text("Not changed in the demo");
            } else if r.clicked() {
                match crate::sys::set_startup(crate::sys::RUN_VALUE, !startup) {
                    Ok(()) => page.startup = Some(!startup),
                    Err(e) => {
                        toast = Some((format!("Could not change the startup entry: {e}"), true))
                    }
                }
            }
        },
    );
    widgets::setting_row(
        ui,
        "Keep running when closed",
        "Closing the window leaves DisplayForge in the notification area, for the quick panel, hotkeys and the schedule.",
        |ui| {
            if widgets::toggle(ui, s.close_to_tray, true).clicked() {
                s.close_to_tray = !s.close_to_tray;
            }
        },
    );
    widgets::setting_row(
        ui,
        "Mouse wheel on the tray icon",
        "Scroll over the icon to change brightness.",
        |ui| {
            if widgets::toggle(ui, s.scroll_on_tray, true).clicked() {
                s.scroll_on_tray = !s.scroll_on_tray;
            }
        },
    );
    if let Some((t, e)) = toast {
        core.toast(t, e);
    }
}

fn brightness_card(ui: &mut Ui, core: &mut Core) {
    widgets::section(ui, "Brightness");
    let s = &mut core.settings;
    widgets::setting_row(
        ui,
        "Fade changes",
        "Brightness glides to a new value instead of jumping.",
        |ui| {
            if widgets::toggle(ui, s.fade, true).clicked() {
                s.fade = !s.fade;
            }
        },
    );
    widgets::setting_row(ui, "Fade length", "", |ui| {
        ui.add_enabled(
            s.fade,
            egui::DragValue::new(&mut s.fade_ms)
                .range(100..=3000)
                .speed(10.0)
                .suffix(" ms"),
        );
        let current = match s.fade_ms {
            0..=250 => 0,
            251..=600 => 1,
            _ => 2,
        };
        if s.fade {
            if let Some(i) = widgets::segmented(ui, &["Quick", "Normal", "Slow"], current) {
                s.fade_ms = [200, 400, 900][i];
            }
        }
    });
    widgets::setting_row(
        ui,
        "Step",
        "How much a hotkey or one wheel notch changes brightness.",
        |ui| {
            ui.add(egui::DragValue::new(&mut s.step).range(1..=50).suffix(" %"));
        },
    );
    widgets::setting_row(
        ui,
        "Dim below zero",
        "Sliders go down to -50: past the monitor's lowest level, the picture is darkened in software. Handy at night.",
        |ui| {
            if widgets::toggle(ui, s.below_zero, true).clicked() {
                s.below_zero = !s.below_zero;
            }
        },
    );
    widgets::setting_row(
        ui,
        "Quick panel moves all together",
        "One slider for every linked monitor at the top of the quick panel.",
        |ui| {
            if widgets::toggle(ui, s.link_all, true).clicked() {
                s.link_all = !s.link_all;
            }
        },
    );
    let p = pal(ui);
    ui.label(
        RichText::new("Shown on each monitor's card, when the monitor supports it")
            .size(12.5)
            .color(p.muted),
    );
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(14.0, 6.0);
        for f in [
            Feature::Contrast,
            Feature::Volume,
            Feature::InputSource,
            Feature::PowerMode,
            Feature::ColorPreset,
        ] {
            let mut on = s.show_features.contains(&f);
            if widgets::checkbox(ui, &mut on, f.label()).changed() {
                if on {
                    s.show_features.push(f);
                } else {
                    s.show_features.retain(|x| *x != f);
                }
            }
        }
    });
}

fn presets_card(ui: &mut Ui, core: &mut Core) {
    let p = pal(ui);
    let mut add = false;
    widgets::split_row(
        ui,
        30.0,
        |ui| widgets::section(ui, "Presets"),
        |ui| {
            if widgets::small_button(ui, &Icon::ADD.with("Add")).clicked() {
                add = true;
            }
        },
    );
    widgets::hint(
        ui,
        "One click in the quick panel, a hotkey or a schedule point applies a preset to every monitor.",
    );
    let min = core.settings.min_brightness();
    let mut remove = None;
    let n = core.settings.presets.len();
    for (i, pr) in core.settings.presets.iter_mut().enumerate() {
        ui.push_id(("preset", i), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add(
                    egui::TextEdit::singleline(&mut pr.name)
                        .desired_width(92.0)
                        .font(semibold(14.0)),
                );
                ui.add(
                    egui::DragValue::new(&mut pr.brightness)
                        .range(min..=100)
                        .suffix(" %"),
                )
                .on_hover_text("Brightness");
                let mut has = pr.contrast.is_some();
                if widgets::checkbox(ui, &mut has, "Contrast").changed() {
                    pr.contrast = has.then_some(70);
                }
                if let Some(c) = &mut pr.contrast {
                    ui.add(egui::DragValue::new(c).range(0..=100).suffix(" %"));
                }
                let mut has = pr.kelvin.is_some();
                if widgets::checkbox(ui, &mut has, "Night")
                    .on_hover_text("Turn night light on at this colour; unticked turns it off")
                    .changed()
                {
                    pr.kelvin = has.then_some(4500);
                }
                if let Some(k) = &mut pr.kelvin {
                    ui.add(
                        egui::DragValue::new(k)
                            .range(KELVIN_MIN..=KELVIN_NEUTRAL)
                            .speed(25.0)
                            .suffix(" K"),
                    );
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 14.0), Sense::hover());
                    ui.painter()
                        .circle_filled(r.center(), 5.0, kelvin_color(*k));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button_enabled(ui, Icon::DELETE, "Remove this preset", n > 1)
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            });
        });
    }
    if let Some(i) = remove {
        let gone = core.settings.presets.remove(i);
        core.toast(format!("Removed the preset {}", gone.name), false);
    }
    if add {
        let mut k = core.settings.presets.len() + 1;
        let mut name = format!("Preset {k}");
        while core.settings.preset(&name).is_some() {
            k += 1;
            name = format!("Preset {k}");
        }
        core.settings.presets.push(Preset {
            name,
            brightness: 60,
            contrast: None,
            kelvin: None,
            per_monitor: BTreeMap::new(),
        });
    }
    let names: Vec<String> = core
        .settings
        .presets
        .iter()
        .map(|p| p.name.trim().to_lowercase())
        .collect();
    let dup = names
        .iter()
        .enumerate()
        .any(|(i, n)| n.is_empty() || names[..i].contains(n));
    if dup {
        ui.label(
            RichText::new("Each preset needs its own name.")
                .size(12.5)
                .color(p.danger),
        );
    }
}

fn hotkeys_card(ui: &mut Ui, core: &mut Core, page: &mut SettingsPage) {
    let p = pal(ui);
    widgets::section_icon(ui, Icon::KEYBOARD, "Hotkeys");
    widgets::hint(
        ui,
        "Work everywhere in Windows. Click a key combination, then press the new one (Esc cancels, Delete clears).",
    );
    // Recording: the next key pressed with its modifiers.
    if let Some(row) = page.recording {
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        if let Some((key, mods)) = pressed {
            match key {
                egui::Key::Escape => page.recording = None,
                egui::Key::Delete | egui::Key::Backspace if !mods.ctrl && !mods.alt => {
                    if let Some(h) = core.settings.hotkeys.get_mut(row) {
                        h.keys.clear();
                        h.enabled = false;
                    }
                    page.recording = None;
                    page.hotkey_error = None;
                }
                _ => {
                    if let Some(name) = key_name(key) {
                        let text = combo_text(mods.ctrl, mods.alt, mods.shift, &name);
                        let others: Vec<String> = core
                            .settings
                            .hotkeys
                            .iter()
                            .enumerate()
                            .filter(|(i, h)| *i != row && h.enabled && !h.keys.is_empty())
                            .map(|(_, h)| h.keys.clone())
                            .collect();
                        match hotkey_problem(&text, &others) {
                            Some(problem) => page.hotkey_error = Some((row, problem)),
                            None => {
                                if let Some(h) = core.settings.hotkeys.get_mut(row) {
                                    h.keys = text;
                                    h.enabled = true;
                                }
                                page.hotkey_error = None;
                                page.recording = None;
                            }
                        }
                    }
                }
            }
        }
    }
    for (i, h) in core.settings.hotkeys.iter_mut().enumerate() {
        let recording = page.recording == Some(i);
        let label = h.action.label();
        let mut toggle_click = false;
        let mut record_click = false;
        widgets::split_row(
            ui,
            34.0,
            |ui| {
                toggle_click = widgets::toggle(ui, h.enabled, !h.keys.is_empty()).clicked();
                ui.label(RichText::new(label).color(if h.enabled { p.text } else { p.muted }));
            },
            |ui| {
                let text = if recording {
                    "Press the keys…".to_string()
                } else if h.keys.is_empty() {
                    "Not set".to_string()
                } else {
                    h.keys.clone()
                };
                let color = if recording { p.accent } else { p.text };
                let b = egui::Button::new(RichText::new(text).font(semibold(13.5)).color(color))
                    .fill(if recording {
                        p.accent.gamma_multiply(0.12)
                    } else {
                        p.hover
                    })
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        if recording { p.accent } else { p.border },
                    ))
                    .corner_radius(egui::CornerRadius::same(7))
                    .min_size(vec2(150.0, 28.0));
                record_click = ui
                    .add(b)
                    .on_hover_text("Click, then press the new keys")
                    .clicked();
            },
        );
        if toggle_click {
            h.enabled = !h.enabled;
        }
        if record_click {
            page.recording = if recording { None } else { Some(i) };
            page.hotkey_error = None;
        }
        if let Some((row, msg)) = &page.hotkey_error {
            if *row == i {
                ui.label(RichText::new(msg).size(12.5).color(p.danger));
            }
        }
    }
    // Two enabled hotkeys with the same keys.
    let mut seen: Vec<String> = Vec::new();
    for h in core
        .settings
        .hotkeys
        .iter()
        .filter(|h| h.enabled && !h.keys.is_empty())
    {
        let k = h.keys.to_ascii_lowercase();
        if seen.contains(&k) {
            ui.label(
                RichText::new(format!(
                    "{} is used twice; only the first one works.",
                    h.keys
                ))
                .size(12.5)
                .color(p.danger),
            );
        }
        seen.push(k);
    }
}

fn file_card(ui: &mut Ui, core: &mut Core) {
    let p = pal(ui);
    widgets::section(ui, "Settings file");
    widgets::hint(
        ui,
        "Save your presets, schedule, profiles and hotkeys to a file, or bring them over from another PC.",
    );
    let demo = core.demo;
    let mut export = false;
    let mut import = false;
    let mut folder = false;
    ui.horizontal(|ui| {
        export = widgets::ghost_button(ui, &Icon::EXPORT.with("Export…")).clicked();
        import = widgets::ghost_button(ui, &Icon::IMPORT.with("Import…")).clicked();
        folder = widgets::ghost_button(ui, &Icon::OPEN_WINDOW.with("Open folder"))
            .on_hover_text(core.shared.dir.display().to_string())
            .clicked();
    });
    ui.label(
        RichText::new(core.shared.dir.display().to_string())
            .size(12.0)
            .color(p.faint),
    );
    if (export || import || folder) && demo {
        core.toast(
            "Not available in the demo: nothing is read or written",
            false,
        );
        return;
    }
    if export {
        let file = rfd::FileDialog::new()
            .set_title("Export DisplayForge settings")
            .set_file_name("DisplayForge settings.json")
            .add_filter("Settings", &["json"])
            .save_file();
        if let Some(path) = file {
            match std::fs::write(&path, core.settings.to_json()) {
                Ok(()) => core.toast(format!("Saved to {}", path.display()), false),
                Err(e) => core.toast(format!("Could not save the file: {e}"), true),
            }
        }
    }
    if import {
        let file = rfd::FileDialog::new()
            .set_title("Import DisplayForge settings")
            .add_filter("Settings", &["json"])
            .pick_file();
        if let Some(path) = file {
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| Settings::from_json(&t))
            {
                Ok(s) => {
                    core.settings = s;
                    core.toast("Settings imported", false);
                }
                Err(e) => core.toast(
                    format!("That file could not be read as settings: {e}"),
                    true,
                ),
            }
        }
    }
    if folder {
        let _ = std::fs::create_dir_all(&core.shared.dir);
        crate::sys::open_path(&core.shared.dir);
    }
}

/// Returns true when "More" was clicked.
fn about_card(ui: &mut Ui, logo: Option<&egui::TextureHandle>) -> bool {
    let p = pal(ui);
    let mut more = false;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
        paint_logo(ui, logo, rect);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new("DisplayForge").font(semibold(17.0)));
            ui.label(
                RichText::new(format!(
                    "Version {}  ·  MIT License",
                    env!("CARGO_PKG_VERSION")
                ))
                .size(12.5)
                .color(p.muted),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("By Zheer Khalid").size(12.5));
                ui.hyperlink_to(
                    RichText::new("(zheer.dev)").size(12.5).color(p.accent),
                    "https://zheer.dev",
                );
            });
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            more = widgets::small_button(ui, "About").clicked();
        });
    });
    ui.label(
        RichText::new(
            Icon::LOCK.with("Makes no network requests. Settings stay in a file on this PC."),
        )
        .size(12.5)
        .color(p.good),
    );
    more
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_keys_become_hotkey_text() {
        assert_eq!(key_name(egui::Key::ArrowUp).as_deref(), Some("Up"));
        assert_eq!(key_name(egui::Key::A).as_deref(), Some("A"));
        assert_eq!(key_name(egui::Key::Num1).as_deref(), Some("1"));
        assert_eq!(key_name(egui::Key::F12).as_deref(), Some("F12"));
        assert_eq!(key_name(egui::Key::End).as_deref(), Some("End"));
        assert_eq!(key_name(egui::Key::Escape), None);
        assert_eq!(combo_text(true, true, false, "Up"), "Ctrl+Alt+Up");
        assert_eq!(combo_text(false, false, true, "F5"), "Shift+F5");
    }

    #[test]
    fn hotkeys_need_a_modifier_and_must_be_unique() {
        assert!(hotkey_problem("A", &[]).is_some());
        assert!(hotkey_problem("Shift+A", &[]).is_some());
        let others = vec!["Ctrl+Alt+Up".to_string()];
        let dup = hotkey_problem("ctrl+alt+up", &others).unwrap();
        assert!(dup.contains("already used"), "{dup}");
    }
}
