//! Build script: renders the DisplayForge icon (vector, via tiny-skia) into a
//! multi-size .ico and a PNG for the window, then embeds the icon, manifest and version info.

#![allow(clippy::field_reassign_with_default)]

use std::{env, fs, path::PathBuf};
use tiny_skia::*;

fn rounded_rect(x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> Path {
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.quad_to(x1, y0, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.quad_to(x1, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.quad_to(x0, y1, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.quad_to(x0, y0, x0 + r, y0);
    pb.close();
    pb.finish().expect("rounded rect")
}

fn render_icon(size: u32) -> Vec<u8> {
    let mut pm = Pixmap::new(size, size).expect("pixmap");
    let s = size as f32 / 256.0;
    let id = Transform::identity();

    // Background: rounded square, cobalt blue -> sunflower yellow (dusk to day).
    let (x0, y0, x1, y1) = (8.0 * s, 8.0 * s, 248.0 * s, 248.0 * s);
    let bg = rounded_rect(x0, y0, x1, y1, 56.0 * s);
    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.shader = LinearGradient::new(
        Point::from_xy(x0, y0),
        Point::from_xy(x1, y1),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(0x1f, 0x4f, 0xd6, 255)),
            GradientStop::new(0.45, Color::from_rgba8(0x3f, 0x86, 0xf0, 255)),
            GradientStop::new(0.75, Color::from_rgba8(0xd0, 0x7a, 0xa8, 255)),
            GradientStop::new(1.0, Color::from_rgba8(0xff, 0xb8, 0x2e, 255)),
        ],
        SpreadMode::Pad,
        id,
    )
    .expect("gradient");
    pm.fill_path(&bg, &paint, FillRule::Winding, id, None);

    let mut white = Paint::default();
    white.anti_alias = true;
    white.set_color_rgba8(255, 255, 255, 255);
    let round = |width: f32| Stroke {
        width: width * s,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    let small = size < 32;

    // The monitor: a screen outline on a short stand.
    let (sx0, sy0, sx1, sy1) = (48.0, 58.0, 208.0, 166.0);
    let screen = rounded_rect(sx0 * s, sy0 * s, sx1 * s, sy1 * s, 16.0 * s);
    pm.stroke_path(
        &screen,
        &white,
        &round(if small { 22.0 } else { 15.0 }),
        id,
        None,
    );
    let mut pb = PathBuilder::new();
    pb.move_to(128.0 * s, 168.0 * s);
    pb.line_to(128.0 * s, 196.0 * s);
    pb.move_to(92.0 * s, 200.0 * s);
    pb.line_to(164.0 * s, 200.0 * s);
    let stand = pb.finish().expect("stand");
    pm.stroke_path(
        &stand,
        &white,
        &round(if small { 22.0 } else { 15.0 }),
        id,
        None,
    );

    // The sun on the screen: a disc, with rays at the larger sizes.
    let (cx, cy) = (128.0, 112.0);
    let mut pb = PathBuilder::new();
    pb.push_circle(cx * s, cy * s, if small { 24.0 } else { 18.0 } * s);
    let disc = pb.finish().expect("sun");
    pm.fill_path(&disc, &white, FillRule::Winding, id, None);
    if !small {
        let mut pb = PathBuilder::new();
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::PI / 4.0;
            let (c, si) = (a.cos(), a.sin());
            pb.move_to((cx + c * 28.0) * s, (cy + si * 28.0) * s);
            pb.line_to((cx + c * 38.0) * s, (cy + si * 38.0) * s);
        }
        let rays = pb.finish().expect("rays");
        pm.stroke_path(&rays, &white, &round(9.0), id, None);
    }

    pm.encode_png().expect("png")
}

fn write_ico(path: &PathBuf, sizes: &[u32]) {
    let pngs: Vec<Vec<u8>> = sizes.iter().map(|&s| render_icon(s)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (i, &s) in sizes.iter().enumerate() {
        let b = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[b, b, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(pngs[i].len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += pngs[i].len() as u32;
    }
    for p in &pngs {
        out.extend_from_slice(p);
    }
    fs::write(path, out).expect("write ico");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let ico = out.join("displayforge.ico");
    write_ico(&ico, &[16, 20, 24, 32, 40, 48, 64, 256]);
    let png = render_icon(256);
    fs::write(out.join("icon-256.png"), &png).expect("write png");
    let docs_png = PathBuf::from("docs/assets/displayforge-icon.png");
    if fs::read(&docs_png).ok().as_deref() != Some(&png[..]) {
        let _ = fs::create_dir_all("docs/assets");
        let _ = fs::write(&docs_png, &png);
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ver = env::var("CARGO_PKG_VERSION").expect("version");
    let parts: Vec<u16> = ver
        .split(['.', '-'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect();
    let (ma, mi, pa) = (
        parts[0],
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    );
    let ver4 = format!("{ma}.{mi}.{pa}.0");
    println!("cargo:rerun-if-changed=res/displayforge.manifest.in");
    let manifest = fs::read_to_string("res/displayforge.manifest.in")
        .expect("manifest")
        .replace("@VERSION4@", &ver4);
    let manifest_path = out.join("displayforge.manifest");
    fs::write(&manifest_path, manifest).expect("write manifest");
    let esc = |p: &PathBuf| p.display().to_string().replace('\\', "\\\\");
    let rc = format!(
        r#"#pragma code_page(65001)
1 ICON "{ico}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {ma},{mi},{pa},0
PRODUCTVERSION {ma},{mi},{pa},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "Zheer Khalid (zheer.dev)"
      VALUE "FileDescription", "DisplayForge monitor control"
      VALUE "FileVersion", "{ver}"
      VALUE "InternalName", "DisplayForge"
      VALUE "LegalCopyright", "Copyright (c) 2026 Zheer Khalid. MIT License."
      VALUE "OriginalFilename", "DisplayForge.exe"
      VALUE "ProductName", "DisplayForge"
      VALUE "ProductVersion", "{ver}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        ico = esc(&ico),
        manifest = esc(&manifest_path),
    );
    let rc_path = out.join("displayforge.rc");
    fs::write(&rc_path, rc).expect("write rc");
    embed_resource::compile(&rc_path, embed_resource::NONE);
    println!("cargo:rustc-link-arg-bins=/MANIFEST:NO");
}
