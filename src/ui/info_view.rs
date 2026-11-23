//! The Monitor info page: what each monitor reports about itself (EDID),
//! what it supports over DDC/CI, and a read-only list of all its settings.

use super::app::{page_header, App};
use super::core::Core;
use super::monitors::{choice_name, connection_color, connection_hint, monitor_icon};
use super::theme::{pal, semibold, Icon};
use super::widgets;
use crate::engine::{Cmd, MonitorView};
use crate::model::Feature;
use eframe::egui::{self, vec2, RichText, Ui};
use std::time::Instant;

#[derive(Default)]
pub struct InfoPage {
    selected: usize,
    /// Read all settings of the first monitor with DDC/CI when the page opens (screenshots).
    pub read_on_open: bool,
    requested: Option<(String, Instant)>,
}

/// The MCCS name of a VCP code.
pub fn vcp_name(code: u8) -> &'static str {
    match code {
        0x02 => "New control value",
        0x04 => "Restore factory defaults",
        0x05 => "Restore brightness and contrast",
        0x06 => "Restore geometry",
        0x08 => "Restore colour",
        0x0B => "Colour temperature step",
        0x0C => "Colour temperature",
        0x10 => "Brightness",
        0x12 => "Contrast",
        0x14 => "Colour preset",
        0x16 => "Red gain",
        0x18 => "Green gain",
        0x1A => "Blue gain",
        0x52 => "Active control",
        0x60 => "Input source",
        0x62 => "Speaker volume",
        0x6C => "Red black level",
        0x6E => "Green black level",
        0x70 => "Blue black level",
        0x87 => "Sharpness",
        0x8D => "Audio mute",
        0xAA => "Screen orientation",
        0xAC => "Horizontal frequency",
        0xAE => "Vertical frequency",
        0xB2 => "Sub-pixel layout",
        0xB6 => "Display technology",
        0xC0 => "Usage time",
        0xC6 => "Application key",
        0xC8 => "Controller type",
        0xC9 => "Firmware level",
        0xCA => "On-screen menu",
        0xCC => "Menu language",
        0xD6 => "Power mode",
        0xDC => "Display mode",
        0xDF => "MCCS version",
        _ => "Manufacturer specific or other",
    }
}

/// The value list of a code in words, where DisplayForge knows the names.
fn values_text(code: u8, values: &[u16]) -> String {
    if values.is_empty() {
        return "Continuous".into();
    }
    let feature = Feature::from_vcp(code).filter(|f| !f.continuous());
    values
        .iter()
        .map(|v| match feature {
            Some(f) => choice_name(f, *v),
            None => format!("0x{v:02X}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// "60 x 34 cm (27.2 in)".
pub fn size_text(width_cm: u8, height_cm: u8, inches: Option<f32>) -> String {
    match inches {
        Some(d) => format!("{width_cm} x {height_cm} cm ({d:.1} in)"),
        None => "Not reported (a projector or TV, perhaps)".into(),
    }
}

impl App {
    pub(super) fn info_ui(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let monitors = self.core.live.monitors.clone();
        let mut show = self.core.settings.show_serials;
        page_header(
            ui,
            "Monitor info",
            "What each monitor reports about itself",
            |ui| {
                if widgets::checkbox(ui, &mut show, "Show serial numbers")
                    .on_hover_text("Serial numbers are hidden so screenshots can be shared safely")
                    .changed()
                {}
            },
        );
        self.core.settings.show_serials = show;
        if monitors.is_empty() {
            widgets::card(ui, |ui| {
                widgets::empty_state(
                    ui,
                    Icon::MONITOR,
                    p.muted,
                    "No monitor found",
                    "Connect a monitor, then look again with the refresh button at the top.",
                );
            });
            return;
        }
        if self.info.selected >= monitors.len() {
            self.info.selected = 0;
        }
        if self.info.read_on_open {
            self.info.read_on_open = false;
            if let Some(i) = monitors
                .iter()
                .position(|m| m.monitor.caps.is_some() && m.monitor.primary)
                .or_else(|| monitors.iter().position(|m| m.monitor.caps.is_some()))
            {
                self.info.selected = i;
                let key = monitors[i].monitor.key.clone();
                self.core.send(Cmd::ReadAllVcp(key.clone()));
                self.info.requested = Some((key, Instant::now()));
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for (i, m) in monitors.iter().enumerate() {
                let label = format!(
                    "{}  {}. {}",
                    monitor_icon(m.monitor.connection).glyph(),
                    m.monitor.number,
                    self.core.monitor_name(m)
                );
                if widgets::chip(ui, label.trim(), i == self.info.selected).clicked() {
                    self.info.selected = i;
                }
            }
        });
        ui.add_space(4.0);
        let m = monitors[self.info.selected].clone();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let core = &self.core;
                widgets::card_row(ui, &[1.0, 1.0], &mut |ui, i| {
                    if i == 0 {
                        identity_card(ui, core, &m);
                    } else {
                        screen_card(ui, core, &m);
                    }
                });
                ui.add_space(widgets::GAP);
                let page = &mut self.info;
                let core = &mut self.core;
                widgets::card_row(ui, &[1.0, 1.0], &mut |ui, i| {
                    if i == 0 {
                        caps_card(ui, &m);
                    } else {
                        dump_card(ui, core, page, &m);
                    }
                });
            });
    }
}

fn identity_card(ui: &mut Ui, core: &Core, m: &MonitorView) {
    let p = pal(ui);
    widgets::section_icon(ui, Icon::INFO, "Identity");
    let Some(e) = &m.monitor.edid else {
        widgets::hint(
            ui,
            "This monitor did not report an EDID, so its details are unknown.",
        );
        return;
    };
    let serial = match &e.serial {
        None => "Not reported".to_string(),
        Some(s) if core.settings.show_serials => s.clone(),
        Some(_) => "Hidden".to_string(),
    };
    let made = match (e.week, e.year) {
        (Some(w), Some(y)) if w > 0 && w <= 53 => format!("Week {w} of {y}"),
        (_, Some(y)) => y.to_string(),
        _ => "Not reported".into(),
    };
    let items = [
        (
            "Maker",
            if e.manufacturer == e.manufacturer_id {
                e.manufacturer.clone()
            } else {
                format!("{} ({})", e.manufacturer, e.manufacturer_id)
            },
        ),
        (
            "Model",
            e.name.clone().unwrap_or_else(|| m.monitor.name.clone()),
        ),
        ("Product code", format!("0x{:04X}", e.product_code)),
        ("Serial number", serial),
        ("Made", made),
        ("EDID version", format!("{}.{}", e.version.0, e.version.1)),
        (
            "Signal",
            if e.digital { "Digital" } else { "Analogue" }.to_string(),
        ),
        ("Windows device", m.monitor.device.clone()),
    ];
    widgets::field_grid(ui, &format!("id-{}", m.monitor.key), 2, &items);
    if !core.settings.show_serials && e.serial.is_some() {
        ui.label(
            RichText::new("Tick “Show serial numbers” at the top to see it.")
                .size(12.0)
                .color(p.faint),
        );
    }
}

fn screen_card(ui: &mut Ui, core: &Core, m: &MonitorView) {
    let p = pal(ui);
    widgets::section_icon(
        ui,
        monitor_icon(m.monitor.connection),
        "Screen and connection",
    );
    let display = core
        .live
        .displays
        .iter()
        .find(|d| d.device == m.monitor.device);
    let mut items: Vec<(&str, String)> = Vec::new();
    if let Some(e) = &m.monitor.edid {
        items.push((
            "Size",
            size_text(e.width_cm, e.height_cm, e.diagonal_inches()),
        ));
        items.push((
            "Native mode",
            e.native
                .map(|(w, h, hz)| format!("{w} x {h} at {hz:.0} Hz"))
                .unwrap_or_else(|| "Not reported".into()),
        ));
        items.push((
            "Colour depth",
            e.bit_depth
                .map(|b| format!("{b} bits per colour"))
                .unwrap_or_else(|| "Not reported".into()),
        ));
    }
    if let Some(d) = display {
        items.push((
            "Current mode",
            format!("{} at {} Hz", d.current.label(), d.current.refresh),
        ));
        items.push(("Scaling", format!("{} %", d.scale_percent)));
    }
    items.push(("Control", m.monitor.connection.label().to_string()));
    items.push((
        "Position",
        format!(
            "{}, {} ({} x {})",
            m.monitor.rect.x, m.monitor.rect.y, m.monitor.rect.w, m.monitor.rect.h
        ),
    ));
    items.push((
        "Main display",
        if m.monitor.primary { "Yes" } else { "No" }.to_string(),
    ));
    widgets::field_grid(ui, &format!("scr-{}", m.monitor.key), 2, &items);
    ui.horizontal(|ui| {
        widgets::badge(
            ui,
            m.monitor.connection.label(),
            connection_color(ui, m.monitor.connection),
        );
        ui.add(
            egui::Label::new(
                RichText::new(connection_hint(m.monitor.connection))
                    .size(12.5)
                    .color(p.muted),
            )
            .wrap(),
        );
    });
}

fn caps_card(ui: &mut Ui, m: &MonitorView) {
    let p = pal(ui);
    widgets::section_icon(ui, Icon::CHIP, "DDC/CI capabilities");
    let Some(caps) = &m.monitor.caps else {
        widgets::hint(
            ui,
            match m.monitor.connection {
                crate::model::Connection::Internal => {
                    "A built-in screen is controlled through Windows, not DDC/CI."
                }
                _ => "This monitor did not report its capabilities over DDC/CI. If it has a DDC/CI option in its own menu, turn it on.",
            },
        );
        return;
    };
    let items = [
        (
            "MCCS version",
            caps.mccs_version.clone().unwrap_or_else(|| "–".into()),
        ),
        ("Type", caps.kind.clone().unwrap_or_else(|| "–".into())),
        ("Model", caps.model.clone().unwrap_or_else(|| "–".into())),
        ("Codes listed", caps.vcp.len().to_string()),
    ];
    widgets::field_grid(ui, &format!("caps-{}", m.monitor.key), 4, &items);
    ui.add_space(2.0);
    egui::Grid::new(("caps-codes", &m.monitor.key))
        .num_columns(3)
        .spacing(vec2(14.0, 5.0))
        .striped(false)
        .show(ui, |ui| {
            for h in ["Code", "Setting", "Values"] {
                ui.label(RichText::new(h).size(12.0).color(p.muted));
            }
            ui.end_row();
            for (code, values) in &caps.vcp {
                ui.label(
                    RichText::new(format!("0x{code:02X}"))
                        .monospace()
                        .color(p.muted),
                );
                ui.label(vcp_name(*code));
                ui.add(
                    egui::Label::new(RichText::new(values_text(*code, values)).size(13.0)).wrap(),
                );
                ui.end_row();
            }
        });
}

fn dump_card(ui: &mut Ui, core: &mut Core, page: &mut InfoPage, m: &MonitorView) {
    let p = pal(ui);
    let key = m.monitor.key.clone();
    let can = m.monitor.caps.is_some();
    let mut read = false;
    widgets::split_row(
        ui,
        30.0,
        |ui| widgets::section_icon(ui, Icon::LIST, "All settings (read only)"),
        |ui| {
            if widgets::small_button(ui, &Icon::REFRESH.with("Read all settings"))
                .on_hover_text("Asks the monitor for every value it lists. Nothing is changed.")
                .clicked()
            {
                read = true;
            }
        },
    );
    if read {
        if can {
            core.send(Cmd::ReadAllVcp(key.clone()));
            page.requested = Some((key.clone(), Instant::now()));
        } else {
            core.toast("This monitor does not answer over DDC/CI", true);
        }
    }
    let dump = core
        .live
        .vcp_dump
        .as_ref()
        .filter(|(k, _)| *k == key)
        .map(|(_, v)| v.clone());
    let Some(rows) = dump else {
        let waiting = page
            .requested
            .as_ref()
            .is_some_and(|(k, t)| *k == key && t.elapsed().as_secs() < 20);
        widgets::hint(
            ui,
            if waiting {
                "Reading… each value takes the monitor a moment."
            } else if can {
                "Read every value the monitor lists, for a closer look. This takes a few seconds and changes nothing."
            } else {
                "Only monitors that answer over DDC/CI have settings to read."
            },
        );
        return;
    };
    egui::Grid::new(("dump", &key))
        .num_columns(4)
        .spacing(vec2(14.0, 5.0))
        .show(ui, |ui| {
            for h in ["Code", "Setting", "Value", "Maximum"] {
                ui.label(RichText::new(h).size(12.0).color(p.muted));
            }
            ui.end_row();
            for (code, value) in &rows {
                ui.label(
                    RichText::new(format!("0x{code:02X}"))
                        .monospace()
                        .color(p.muted),
                );
                ui.label(vcp_name(*code));
                match value {
                    Ok(level) => {
                        let shown = match Feature::from_vcp(*code).filter(|f| !f.continuous()) {
                            Some(f) => choice_name(f, level.value as u16),
                            None => level.value.to_string(),
                        };
                        ui.label(RichText::new(shown).font(semibold(13.5)));
                        ui.label(RichText::new(level.max.to_string()).color(p.muted));
                    }
                    Err(e) => {
                        ui.label(RichText::new("Not readable").color(p.warn))
                            .on_hover_text(e.as_str());
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_sizes_read_well() {
        assert_eq!(vcp_name(0x10), "Brightness");
        assert_eq!(vcp_name(0xE9), "Manufacturer specific or other");
        assert_eq!(values_text(0x10, &[]), "Continuous");
        assert_eq!(values_text(0x60, &[0x0F, 0x11]), "DisplayPort 1, HDMI 1");
        assert_eq!(values_text(0xCC, &[0x02]), "0x02");
        assert_eq!(size_text(60, 34, Some(27.15)), "60 x 34 cm (27.1 in)");
        assert!(size_text(0, 0, None).starts_with("Not reported"));
    }
}
