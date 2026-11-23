//! The main window: header, navigation rail, pages, About, toasts, requests
//! from the engine, child windows (quick panel, test patterns, identify) and
//! the screenshot mechanism.

use super::core::Core;
use super::display_view::DisplayPage;
use super::flyout::{self, FlyoutAction, FlyoutWindow};
use super::info_view::InfoPage;
use super::profiles_view::ProfilesPage;
use super::schedule_view::SchedulePage;
use super::settings_view::SettingsPage;
use super::theme::{self, pal, pal_ctx, regular, semibold, Icon};
use super::tools::ToolsPage;
use super::widgets;
use super::Options;
use crate::engine::{Cmd, UiRequest};
use eframe::egui::{
    self, vec2, Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense,
    Stroke, TextureHandle, Ui,
};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Monitors,
    Schedule,
    Profiles,
    Display,
    Info,
    Tools,
    Settings,
}

impl Tab {
    const ALL: [Tab; 7] = [
        Tab::Monitors,
        Tab::Schedule,
        Tab::Profiles,
        Tab::Display,
        Tab::Info,
        Tab::Tools,
        Tab::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Monitors => "Monitors",
            Tab::Schedule => "Schedule",
            Tab::Profiles => "App profiles",
            Tab::Display => "Display",
            Tab::Info => "Monitor info",
            Tab::Tools => "Tools",
            Tab::Settings => "Settings",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Tab::Monitors => Icon::BRIGHTNESS,
            Tab::Schedule => Icon::CLOCK,
            Tab::Profiles => Icon::APPS,
            Tab::Display => Icon::MONITOR,
            Tab::Info => Icon::INFO,
            Tab::Tools => Icon::REPAIR,
            Tab::Settings => Icon::SETTINGS,
        }
    }

    pub fn from_name(name: &str) -> Option<Tab> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "monitors" | "home" | "" => Tab::Monitors,
            "schedule" => Tab::Schedule,
            "profiles" | "apps" => Tab::Profiles,
            "display" | "display-confirm" => Tab::Display,
            "info" | "monitor-info" => Tab::Info,
            "tools" => Tab::Tools,
            "settings" | "about" => Tab::Settings,
            _ => return None,
        })
    }
}

struct Shot {
    path: PathBuf,
    frames: u32,
    requested: bool,
}

pub struct App {
    pub(super) core: Core,
    pub(super) tab: Tab,
    pub(super) about_open: bool,
    pub(super) logo: Option<TextureHandle>,
    shot: Option<Shot>,
    /// The "flyout" screenshot: the quick panel over an invented desktop.
    flyout_backdrop: bool,
    flyout: FlyoutWindow,
    pub(super) schedule: SchedulePage,
    pub(super) profiles: ProfilesPage,
    pub(super) display: DisplayPage,
    pub(super) info: InfoPage,
    pub(super) tools: ToolsPage,
    pub(super) settings_page: SettingsPage,
}

pub fn logo_texture(ctx: &egui::Context, icon: &egui::IconData) -> Option<TextureHandle> {
    (icon.width > 0).then(|| {
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        );
        ctx.load_texture("logo", image, egui::TextureOptions::LINEAR)
    })
}

pub fn paint_logo(ui: &Ui, logo: Option<&TextureHandle>, rect: Rect) {
    if let Some(logo) = logo {
        ui.painter().image(
            logo.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::COBALT);
    }
}

/// Opens the main window and returns when it is closed.
pub fn run_main(options: Options, icon: egui::IconData) -> eframe::Result<()> {
    let shared = options.shared.clone().unwrap_or_else(crate::demo::shared);
    let title = if shared.demo {
        "DisplayForge (demo)"
    } else {
        "DisplayForge"
    };
    let viewport = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([1040.0, 640.0])
        .with_icon(icon.clone());
    let native = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "DisplayForge",
        native,
        Box::new(move |cc| {
            let options = Options {
                shared: Some(shared),
                ..options
            };
            Ok(Box::new(App::new(cc, options, icon)))
        }),
    )
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, options: Options, icon: egui::IconData) -> Self {
        let ctx = &cc.egui_ctx;
        theme::install(ctx);
        let shared = options.shared.clone().unwrap_or_else(crate::demo::shared);
        let mut core = Core::new(shared, options.control.clone());
        core.forced_light = options.light;
        if options.light {
            theme::apply_choice(ctx, crate::settings::ThemeChoice::Light);
        } else {
            theme::apply_choice(ctx, core.settings.theme);
        }
        let wake_ctx = ctx.clone();
        if let Ok(mut waker) = core.shared.waker.lock() {
            *waker = Some(Box::new(move || wake_ctx.request_repaint()));
        }
        let mut app = App {
            core,
            tab: Tab::Monitors,
            about_open: false,
            logo: logo_texture(ctx, &icon),
            shot: options.screenshot.clone().map(|path| Shot {
                path,
                frames: 0,
                requested: false,
            }),
            flyout_backdrop: false,
            flyout: FlyoutWindow::default(),
            schedule: SchedulePage::default(),
            profiles: ProfilesPage::default(),
            display: DisplayPage::default(),
            info: InfoPage::default(),
            tools: ToolsPage::default(),
            settings_page: SettingsPage::default(),
        };
        app.apply_view(options.view.as_deref().unwrap_or("monitors"));
        app
    }

    /// Opens the page or dialog named on the command line or in a request.
    pub(super) fn apply_view(&mut self, view: &str) {
        let view = view.trim().to_ascii_lowercase();
        match view.as_str() {
            "flyout" => {
                if self.shot.is_some() {
                    self.flyout_backdrop = true;
                } else {
                    self.flyout.open = true;
                }
            }
            "about" => {
                self.tab = Tab::Settings;
                self.about_open = true;
            }
            "display-confirm" => {
                self.tab = Tab::Display;
                self.display.demo_confirm(&self.core, self.shot.is_some());
            }
            "info" => {
                self.tab = Tab::Info;
                self.info.read_on_open = true;
            }
            v => {
                if let Some(tab) = Tab::from_name(v) {
                    self.tab = tab;
                }
            }
        }
    }

    fn handle_requests(&mut self, ctx: &egui::Context) {
        if self.core.shared.quit.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.shot.is_some() {
            return;
        }
        for request in self.core.shared.take_requests() {
            match request {
                UiRequest::Open(view) => {
                    if let Some(view) = view {
                        self.apply_view(&view);
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                UiRequest::Flyout => {
                    self.flyout = FlyoutWindow::new_open();
                }
                UiRequest::Quit => {
                    self.core.shared.quit.store(true, Ordering::SeqCst);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(shot) = &mut self.shot else { return };
        ctx.request_repaint();
        shot.frames += 1;
        if shot.frames == 1 {
            ctx.set_pixels_per_point(1.0);
        }
        if shot.frames == 3 {
            // Now that one point is one pixel, this gives an exact 1280 x 800 image.
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 800.0)));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let [w, h] = image.size;
            if let Some(parent) = shot.path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let result = image::save_buffer(
                &shot.path,
                image.as_raw(),
                w as u32,
                h as u32,
                image::ColorType::Rgba8,
            );
            if let Err(e) = result {
                eprintln!("screenshot failed: {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.shot = None;
            return;
        }
        if !shot.requested && shot.frames > 40 {
            shot.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        // A safety net: never hang around if the screenshot does not arrive.
        if shot.frames > 400 {
            eprintln!("screenshot timed out");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    // ----------------------------------------------------------------------------------
    // Frame: header and navigation
    // ----------------------------------------------------------------------------------

    fn status_pill(&self, ui: &mut Ui) {
        let p = pal(ui);
        let live = &self.core.live;
        let (color, text, tip) = if !live.ready {
            (
                p.faint,
                "Looking for monitors".to_string(),
                "DisplayForge is asking the monitors what they support.",
            )
        } else if let Some(app) = &live.active_profile {
            (
                p.info,
                format!("Profile  ·  {app}"),
                "An app profile is active while that app is in front.",
            )
        } else if live.idle_dimmed {
            (
                p.warn,
                "Dimmed while away".to_string(),
                "No input for a while: the screens are dimmed until you are back.",
            )
        } else if live.schedule.enabled && live.schedule.following {
            (
                p.good,
                "Following the schedule".to_string(),
                "Brightness and night light follow the daily schedule.",
            )
        } else if live.schedule.enabled {
            (
                p.warn,
                "Schedule paused".to_string(),
                "Paused after a manual change, until the next schedule point.",
            )
        } else {
            (
                p.faint,
                "Manual".to_string(),
                "No schedule: values stay as you set them.",
            )
        };
        let galley = widgets::elided(ui, &text, semibold(12.5), color, 230.0);
        let size = vec2(galley.size().x + 34.0, 26.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(13), color.gamma_multiply(0.14));
        ui.painter()
            .circle_filled(rect.left_center() + vec2(13.0, 0.0), 4.0, color);
        ui.painter().galley(
            egui::pos2(rect.left() + 23.0, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
        resp.on_hover_text(tip);
    }

    fn header(&mut self, ctx: &egui::Context) {
        let p = pal_ctx(ctx);
        egui::TopBottomPanel::top("header")
            .exact_height(64.0)
            .frame(
                Frame::new()
                    .fill(p.rail)
                    .inner_margin(Margin::symmetric(18, 0)),
            )
            .show(ctx, |ui| {
                let full = ui.max_rect();
                ui.painter().hline(
                    full.x_range().expand(18.0),
                    full.bottom(),
                    Stroke::new(1.0_f32, p.border),
                );
                ui.horizontal_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                    paint_logo(ui, self.logo.as_ref(), rect);
                    ui.add_space(2.0);
                    ui.label(RichText::new("DisplayForge").font(semibold(19.0)));
                    if self.core.demo {
                        widgets::badge(ui, "DEMO", p.info).on_hover_text(
                            "Invented monitors. Nothing on this PC is changed or saved.",
                        );
                    }
                    ui.add_space(196.0 - ui.min_rect().width().min(196.0));
                    self.status_pill(ui);

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::icon_button(ui, Icon::INFO, "About DisplayForge").clicked() {
                            self.about_open = true;
                        }
                        if widgets::icon_button(
                            ui,
                            Icon::REFRESH,
                            "Look for monitors again (after plugging one in, or if one stopped answering)",
                        )
                        .clicked()
                        {
                            self.core.send(Cmd::Rescan);
                            self.core.toast("Looking for monitors again", false);
                        }
                        if widgets::icon_button(
                            ui,
                            Icon::POWER,
                            "Turn the screens off now (they wake with the mouse or a key)",
                        )
                        .clicked()
                        {
                            self.core.send(Cmd::ScreensOff);
                        }
                        ui.add_space(4.0);
                        if widgets::ghost_button(ui, &Icon::OPEN_WINDOW.with("Quick panel"))
                            .on_hover_text(
                                "The small panel that opens from the notification area icon",
                            )
                            .clicked()
                        {
                            self.flyout = FlyoutWindow::new_open();
                        }
                    });
                });
            });
    }

    fn rail(&mut self, ctx: &egui::Context) {
        let p = pal_ctx(ctx);
        egui::SidePanel::left("rail")
            .exact_width(208.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(p.rail)
                    .inner_margin(Margin::symmetric(10, 14)),
            )
            .show(ctx, |ui| {
                let full = ui.max_rect();
                ui.painter().vline(
                    full.right() + 10.0,
                    full.y_range().expand(14.0),
                    Stroke::new(1.0_f32, p.border),
                );
                ui.spacing_mut().item_spacing.y = 4.0;
                for tab in Tab::ALL {
                    if tab == Tab::Settings {
                        ui.add_space(6.0);
                        widgets::divider(ui);
                        ui.add_space(6.0);
                    }
                    let active = self.tab == tab;
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                    if active {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(9), p.selected);
                        let bar = Rect::from_min_size(
                            rect.left_center() - vec2(0.0, 9.0),
                            vec2(3.0, 18.0),
                        );
                        ui.painter()
                            .rect_filled(bar, CornerRadius::same(2), p.accent);
                    } else if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(9), p.hover);
                    }
                    let fg = if active { p.text } else { p.muted };
                    ui.painter().text(
                        rect.left_center() + vec2(24.0, 0.0),
                        Align2::CENTER_CENTER,
                        tab.icon().glyph(),
                        regular(16.0),
                        if active { p.accent } else { fg },
                    );
                    ui.painter().text(
                        rect.left_center() + vec2(46.0, -1.0),
                        Align2::LEFT_CENTER,
                        tab.label(),
                        if active {
                            semibold(14.5)
                        } else {
                            regular(14.5)
                        },
                        fg,
                    );
                    if resp.clicked() {
                        self.tab = tab;
                    }
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                                .size(12.0)
                                .color(p.faint),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(Icon::LOCK.with("No network requests"))
                                .size(12.0)
                                .color(p.good),
                        )
                        .on_hover_text(
                            "DisplayForge talks only to your monitors and to Windows. It never connects to the internet.",
                        );
                    });
                });
            });
    }

    // ----------------------------------------------------------------------------------
    // About and toasts
    // ----------------------------------------------------------------------------------

    fn about_dialog(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let p = pal_ctx(ctx);
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("about-dialog"))
            .frame(
                Frame::new()
                    .fill(p.card)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::same(28)),
            )
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.vertical_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(84.0, 84.0), Sense::hover());
                    paint_logo(ui, self.logo.as_ref(), rect);
                    ui.add_space(6.0);
                    ui.label(RichText::new("DisplayForge").font(semibold(26.0)));
                    ui.label(
                        RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                            .color(p.muted),
                    );
                    ui.add_space(10.0);
                    ui.label("Brightness, contrast and night light for every screen.");
                    ui.label(
                        RichText::new(
                            "External monitors over DDC/CI, laptop panels through Windows, and software dimming for the rest. It makes no network requests at all.",
                        )
                        .color(p.muted),
                    );
                    ui.add_space(10.0);
                    ui.label(RichText::new("Developed by Zheer Khalid").font(semibold(15.0)));
                    ui.add_space(2.0);
                    ui.hyperlink_to(
                        RichText::new("zheer.dev").color(p.accent),
                        "https://zheer.dev",
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(
                            "Runs with your normal rights. Some monitors switch DDC/CI off in their own menu; turn it on there to control them.",
                        )
                        .size(12.0)
                        .color(p.faint),
                    );
                    ui.label(
                        RichText::new("Free and open source under the MIT License.")
                            .size(12.0)
                            .color(p.faint),
                    );
                    ui.add_space(14.0);
                    if widgets::primary_button(ui, "Close", true).clicked() {
                        close = true;
                    }
                });
            });
        if close || modal.should_close() {
            self.about_open = false;
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        self.core
            .toasts
            .retain(|t| t.born.elapsed().as_secs_f32() < if t.error { 8.0 } else { 3.5 });
        toasts(ctx, &self.core.toasts);
    }

    fn central(&mut self, ctx: &egui::Context) {
        let p = pal_ctx(ctx);
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(24, 18)),
            )
            .show(ctx, |ui| match self.tab {
                Tab::Monitors => self.monitors_ui(ui),
                Tab::Schedule => self.schedule_ui(ui),
                Tab::Profiles => self.profiles_ui(ui),
                Tab::Display => self.display_ui(ui),
                Tab::Info => self.info_ui(ui),
                Tab::Tools => self.tools_ui(ui),
                Tab::Settings => self.settings_ui(ui),
            });
    }

    /// The quick panel as a child window of the main window.
    fn flyout_child(&mut self, ctx: &egui::Context) {
        if !self.flyout.open {
            return;
        }
        let action = self
            .flyout
            .show_child(ctx, &mut self.core, self.logo.as_ref());
        match action {
            Some(FlyoutAction::OpenMain) => {
                self.flyout.open = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            Some(FlyoutAction::OpenSettings) => {
                self.flyout.open = false;
                self.tab = Tab::Settings;
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            Some(FlyoutAction::Close) => self.flyout.open = false,
            None => {}
        }
    }
}

/// The standard page header: title, subtitle and right-aligned actions.
pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str, actions: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(title).font(semibold(24.0)));
            ui.label(RichText::new(subtitle).color(p.muted));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), actions);
    });
    ui.add_space(6.0);
}

/// Paints the toasts at the bottom right.
pub fn toasts(ctx: &egui::Context, list: &[super::core::Toast]) {
    if list.is_empty() {
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    let p = pal_ctx(ctx);
    egui::Area::new(egui::Id::new("toasts"))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-18.0, -18.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                for t in list.iter().rev().take(4) {
                    let color = if t.error { p.danger } else { p.accent };
                    Frame::new()
                        .fill(p.card)
                        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.7)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.horizontal(|ui| {
                                let icon = if t.error { Icon::ERROR } else { Icon::DONE };
                                ui.label(RichText::new(icon.glyph()).color(color));
                                ui.add(egui::Label::new(t.text.as_str()).wrap());
                            });
                        });
                }
            });
        });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.core.begin_frame();
        self.handle_requests(ctx);
        self.handle_screenshot(ctx);
        if !self.core.forced_light {
            let want = self.core.settings.theme;
            if self.settings_page.applied_theme != Some(want) {
                theme::apply_choice(ctx, want);
                self.settings_page.applied_theme = Some(want);
            }
        }

        if self.flyout_backdrop {
            flyout::backdrop_shot(ctx, &mut self.core, self.logo.as_ref());
        } else {
            self.header(ctx);
            self.rail(ctx);
            self.central(ctx);
            self.display_confirm_ui(ctx);
            self.about_dialog(ctx);
            self.flyout_child(ctx);
            self.tools_windows(ctx);
        }
        self.toasts_ui(ctx);

        let saving = self.core.end_frame();
        if saving || self.core.holding() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        if self.shot.is_none() {
            // Requests from the engine and the clock: look now and then even
            // when nothing wakes the window.
            ctx.request_repaint_after(std::time::Duration::from_millis(750));
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // Screenshots must not depend on where the mouse happens to be:
        // no hover highlights, no tooltips.
        if self.shot.is_some() {
            raw_input.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::PointerMoved(_)
                        | egui::Event::MouseMoved(_)
                        | egui::Event::PointerButton { .. }
                        | egui::Event::MouseWheel { .. }
                )
            });
            raw_input.events.push(egui::Event::PointerGone);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.display.revert_on_exit(&mut self.core);
        self.core.flush();
        if let Ok(mut waker) = self.core.shared.waker.lock() {
            *waker = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_names_map_to_pages() {
        assert_eq!(Tab::from_name("monitors"), Some(Tab::Monitors));
        assert_eq!(Tab::from_name("Schedule"), Some(Tab::Schedule));
        assert_eq!(Tab::from_name("display-confirm"), Some(Tab::Display));
        assert_eq!(Tab::from_name("about"), Some(Tab::Settings));
        assert_eq!(Tab::from_name("nonsense"), None);
        for tab in Tab::ALL {
            assert!(!tab.label().is_empty());
        }
    }
}
