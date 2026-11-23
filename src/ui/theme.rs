//! Colours, fonts and icon glyphs.

use crate::settings::ThemeChoice;
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Theme, ThemePreference, Visuals,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The brand pair: cobalt blue (dusk) and sunflower (day).
pub const COBALT: Color32 = Color32::from_rgb(0x1f, 0x4f, 0xd6);
pub const SUNFLOWER: Color32 = Color32::from_rgb(0xff, 0xb8, 0x2e);
pub const RED: Color32 = Color32::from_rgb(0xff, 0x5f, 0x62);
pub const YELLOW: Color32 = Color32::from_rgb(0xf0, 0xc5, 0x41);
pub const GREEN: Color32 = Color32::from_rgb(0x3e, 0xcf, 0x8e);

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub rail: Color32,
    pub card: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    /// Sunflower on dark, cobalt on light.
    pub accent: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub warn: Color32,
    /// The other brand colour.
    pub info: Color32,
    pub good: Color32,
}

pub fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            dark,
            bg: Color32::from_rgb(0x10, 0x12, 0x1a),
            rail: Color32::from_rgb(0x0b, 0x0d, 0x14),
            card: Color32::from_rgb(0x17, 0x1a, 0x25),
            hover: Color32::from_rgb(0x1f, 0x23, 0x31),
            selected: Color32::from_rgb(0x30, 0x2a, 0x1a),
            border: Color32::from_rgb(0x2a, 0x2f, 0x40),
            text: Color32::from_rgb(0xec, 0xee, 0xf5),
            muted: Color32::from_rgb(0xa0, 0xa6, 0xb8),
            faint: Color32::from_rgb(0x68, 0x70, 0x86),
            accent: Color32::from_rgb(0xff, 0xc5, 0x3d),
            on_accent: Color32::from_rgb(0x2a, 0x1d, 0x00),
            danger: RED,
            warn: YELLOW,
            info: Color32::from_rgb(0x7f, 0xa2, 0xff),
            good: GREEN,
        }
    } else {
        Palette {
            dark,
            bg: Color32::from_rgb(0xf4, 0xf5, 0xf9),
            rail: Color32::from_rgb(0xff, 0xff, 0xff),
            card: Color32::from_rgb(0xff, 0xff, 0xff),
            hover: Color32::from_rgb(0xec, 0xee, 0xf4),
            selected: Color32::from_rgb(0xe3, 0xeb, 0xfc),
            border: Color32::from_rgb(0xda, 0xdd, 0xe7),
            text: Color32::from_rgb(0x17, 0x1a, 0x26),
            muted: Color32::from_rgb(0x57, 0x5e, 0x72),
            faint: Color32::from_rgb(0x8b, 0x92, 0xa5),
            accent: Color32::from_rgb(0x1f, 0x5f, 0xd6),
            on_accent: Color32::WHITE,
            danger: Color32::from_rgb(0xd9, 0x3a, 0x3f),
            warn: Color32::from_rgb(0xa6, 0x7a, 0x00),
            info: Color32::from_rgb(0xb0, 0x74, 0x00),
            good: Color32::from_rgb(0x1c, 0x9a, 0x63),
        }
    }
}

pub fn pal(ui: &egui::Ui) -> Palette {
    palette(ui.visuals().dark_mode)
}

pub fn pal_ctx(ctx: &egui::Context) -> Palette {
    palette(ctx.style().visuals.dark_mode)
}

fn visuals(dark: bool) -> Visuals {
    let p = palette(dark);
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.extreme_bg_color = if dark {
        Color32::from_rgb(0x0b, 0x0c, 0x12)
    } else {
        Color32::WHITE
    };
    v.faint_bg_color = p.hover;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = if dark {
        Color32::from_rgb(0x6b, 0x55, 0x1c)
    } else {
        Color32::from_rgb(0xc4, 0xd6, 0xfa)
    };
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.danger;
    let r = CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.muted);
    v.widgets.inactive.bg_fill = p.hover;
    v.widgets.inactive.weak_bg_fill = p.hover;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.hovered.bg_fill = p.border;
    v.widgets.hovered.weak_bg_fill = p.border;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.faint);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.active.bg_fill = p.border;
    v.widgets.active.weak_bg_fill = p.border;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.open.bg_fill = p.hover;
    v.widgets.open.weak_bg_fill = p.hover;
    v.widgets.open.bg_stroke = Stroke::new(1.0_f32, p.faint);
    v
}

static ICON_FONT: AtomicBool = AtomicBool::new(false);

fn load_fonts(ctx: &egui::Context) {
    let dir = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:\\Windows".into())
        .join("Fonts");
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, file: &str| -> bool {
        match std::fs::read(dir.join(file)) {
            Ok(bytes) => {
                fonts
                    .font_data
                    .insert(name.to_string(), Arc::new(FontData::from_owned(bytes)));
                true
            }
            Err(_) => false,
        }
    };
    // System fonts are loaded at run time so the program stays small.
    let regular = add("ui", "segoeui.ttf");
    let semibold = add("ui-semibold", "seguisb.ttf");
    let icons = add("ui-icons", "segmdl2.ttf") || add("ui-icons", "SegoeIcons.ttf");
    ICON_FONT.store(icons, Ordering::Relaxed);

    let defaults = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut body = Vec::new();
    if regular {
        body.push("ui".to_string());
    }
    if icons {
        body.push("ui-icons".to_string());
    }
    body.extend(defaults.iter().cloned());
    let mut strong = Vec::new();
    if semibold {
        strong.push("ui-semibold".to_string());
    }
    strong.extend(body.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, body);
    fonts
        .families
        .insert(FontFamily::Name("semibold".into()), strong);
    ctx.set_fonts(fonts);
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// One-time setup: fonts, spacing and both colour schemes.
pub fn install(ctx: &egui::Context) {
    load_fonts(ctx);
    ctx.set_visuals_of(Theme::Dark, visuals(true));
    ctx.set_visuals_of(Theme::Light, visuals(false));
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, semibold(22.0)),
            (TextStyle::Body, regular(14.0)),
            (TextStyle::Button, regular(14.0)),
            (TextStyle::Small, regular(12.0)),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.combo_height = 320.0;
        style.spacing.scroll.bar_width = 10.0;
        style.spacing.scroll.floating = true;
        style.interaction.selectable_labels = false;
        style.visuals.indent_has_left_vline = false;
    });
}

pub fn apply_choice(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_theme(match choice {
        ThemeChoice::Dark => ThemePreference::Dark,
        ThemeChoice::Light => ThemePreference::Light,
        ThemeChoice::System => ThemePreference::System,
    });
}

/// Icon glyphs from the system icon font (Segoe MDL2 Assets). If the font is
/// missing the icons are simply left out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Icon(pub &'static str);

#[allow(dead_code)] // a glyph catalogue
impl Icon {
    pub const SEARCH: Icon = Icon("\u{E721}");
    pub const SETTINGS: Icon = Icon("\u{E713}");
    pub const DELETE: Icon = Icon("\u{E74D}");
    pub const REFRESH: Icon = Icon("\u{E72C}");
    pub const APPS: Icon = Icon("\u{E71D}");
    pub const WARNING: Icon = Icon("\u{E7BA}");
    pub const INFO: Icon = Icon("\u{E946}");
    pub const CHECK: Icon = Icon("\u{E73E}");
    pub const CLOSE: Icon = Icon("\u{E711}");
    pub const POWER: Icon = Icon("\u{E7E8}");
    pub const REPAIR: Icon = Icon("\u{E90F}");
    pub const EXPORT: Icon = Icon("\u{EDE1}");
    pub const IMPORT: Icon = Icon("\u{E8B5}");
    pub const CHEVRON_RIGHT: Icon = Icon("\u{E76C}");
    pub const CHEVRON_DOWN: Icon = Icon("\u{E70D}");
    pub const LINK: Icon = Icon("\u{E71B}");
    pub const CLOCK: Icon = Icon("\u{E823}");
    pub const DONE: Icon = Icon("\u{E930}");
    pub const ERROR: Icon = Icon("\u{EA39}");
    pub const UNDO: Icon = Icon("\u{E7A7}");
    pub const PLAY: Icon = Icon("\u{E768}");
    pub const PAUSE: Icon = Icon("\u{E769}");
    pub const ADD: Icon = Icon("\u{E710}");
    pub const EDIT: Icon = Icon("\u{E70F}");
    pub const HIDE: Icon = Icon("\u{ED1A}");
    pub const VIEW: Icon = Icon("\u{E890}");
    pub const UP: Icon = Icon("\u{E74A}");
    pub const DOWN: Icon = Icon("\u{E74B}");
    pub const MONITOR: Icon = Icon("\u{E7F4}");
    pub const LAPTOP: Icon = Icon("\u{E7F8}");
    pub const BRIGHTNESS: Icon = Icon("\u{E706}");
    pub const DIM: Icon = Icon("\u{EC8A}");
    pub const MOON: Icon = Icon("\u{E708}");
    pub const CONTRAST: Icon = Icon("\u{F08C}");
    pub const VOLUME: Icon = Icon("\u{E767}");
    pub const INPUT: Icon = Icon("\u{E8AB}");
    pub const COLOR: Icon = Icon("\u{E790}");
    pub const FULLSCREEN: Icon = Icon("\u{E740}");
    pub const PIN: Icon = Icon("\u{E718}");
    pub const MAP_PIN: Icon = Icon("\u{E81D}");
    pub const KEYBOARD: Icon = Icon("\u{E765}");
    pub const OPEN_WINDOW: Icon = Icon("\u{E8A7}");
    pub const CHIP: Icon = Icon("\u{E950}");
    pub const LIST: Icon = Icon("\u{EA37}");
    pub const LOCK: Icon = Icon("\u{E72E}");
    pub const RECORD: Icon = Icon("\u{E7C8}");
    pub const WIFI: Icon = Icon("\u{E701}");

    /// The glyph, or an empty string when the icon font is unavailable.
    pub fn glyph(self) -> &'static str {
        if ICON_FONT.load(Ordering::Relaxed) {
            self.0
        } else {
            ""
        }
    }

    /// `"<glyph>  text"` for buttons.
    pub fn with(self, text: &str) -> String {
        if ICON_FONT.load(Ordering::Relaxed) {
            format!("{}  {text}", self.0)
        } else {
            text.to_string()
        }
    }
}

/// An approximate display colour for a colour temperature, for previews
/// (Tanner Helland's fit of the black-body curve).
pub fn kelvin_color(kelvin: u32) -> Color32 {
    let t = kelvin.clamp(1000, 40000) as f32 / 100.0;
    let r = if t <= 66.0 {
        255.0
    } else {
        329.698_73 * (t - 60.0).powf(-0.133_204_76)
    };
    let g = if t <= 66.0 {
        99.470_8 * t.ln() - 161.119_57
    } else {
        288.122_16 * (t - 60.0).powf(-0.075_514_85)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_73 * (t - 10.0).ln() - 305.044_8
    };
    let c = |v: f32| v.clamp(0.0, 255.0) as u8;
    // 6500 K comes out slightly warm in this fit; scale so 6500 K is white.
    let (r, g, b) = (c(r), c(g), c(b));
    let w = [255.0_f32, 254.0, 250.0];
    Color32::from_rgb(
        (r as f32 * 255.0 / w[0]).min(255.0) as u8,
        (g as f32 * 255.0 / w[1]).min(255.0) as u8,
        (b as f32 * 255.0 / w[2]).min(255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kelvin_preview_runs_warm_to_white() {
        let warm = kelvin_color(1900);
        let white = kelvin_color(6500);
        assert_eq!(warm.r(), 255);
        assert!(warm.b() < 80, "1900 K is orange: {warm:?}");
        assert!(white.r() > 245 && white.g() > 245 && white.b() > 240);
        assert!(kelvin_color(4500).b() < kelvin_color(5500).b());
    }
}
