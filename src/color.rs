//! Colour temperature and software dimming as gamma ramps.
//!
//! Colour temperature: the colour of a black body at `kelvin` is computed
//! from Planck's law and an analytic fit of the CIE 1931 colour matching
//! functions (Wyman, Sloan and Shirley, 2013), converted to linear sRGB and
//! divided, channel by channel, by the colour of 6500 K. The result is scaled
//! so its largest channel is 1 and then gamma-encoded (2.2), because the
//! multipliers are applied to the (gamma-encoded) ramp. Everything is smooth
//! in the temperature; 6500 K is exactly white.
use crate::model::GammaRamp;
use crate::settings::{KELVIN_NEUTRAL, MIN_BELOW_ZERO};

/// Lowest and highest temperature [`kelvin_to_rgb`] distinguishes.
pub const KELVIN_RANGE: (u32, u32) = (1000, 12000);

/// Gaussian with different widths left and right of its centre.
fn lobe(x: f64, mu: f64, s1: f64, s2: f64) -> f64 {
    let s = if x < mu { s1 } else { s2 };
    let d = (x - mu) / s;
    (-0.5 * d * d).exp()
}

/// CIE 1931 2-degree colour matching functions (multi-lobe fit), nm.
fn cmf(nm: f64) -> (f64, f64, f64) {
    let x = 1.056 * lobe(nm, 599.8, 37.9, 31.0) + 0.362 * lobe(nm, 442.0, 16.0, 26.7)
        - 0.065 * lobe(nm, 501.1, 20.4, 26.2);
    let y = 0.821 * lobe(nm, 568.8, 46.9, 40.5) + 0.286 * lobe(nm, 530.9, 16.3, 31.1);
    let z = 1.217 * lobe(nm, 437.0, 11.8, 36.0) + 0.681 * lobe(nm, 459.0, 26.0, 13.8);
    (x, y, z)
}

/// Linear sRGB of a black body, normalised to Y = 1.
fn blackbody_linear(kelvin: f64) -> [f64; 3] {
    const C2: f64 = 1.438_776_877e-2; // m K
    let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
    let mut nm: f64 = 380.0;
    while nm <= 780.0 {
        let l = nm * 1e-9;
        // Planck's law up to a constant factor.
        let b = 1.0 / (l.powi(5) * ((C2 / (l * kelvin)).exp() - 1.0));
        let (cx, cy, cz) = cmf(nm);
        x += b * cx;
        y += b * cy;
        z += b * cz;
        nm += 2.0;
    }
    let (x, z) = (x / y, z / y);
    let y = 1.0;
    [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ]
}

/// RGB multipliers (0..=1) for a colour temperature; 6500 K = (1, 1, 1).
/// Below 6500 K blue and green fall (red stays 1); above it red and green
/// fall (blue stays 1). The temperature is limited to [`KELVIN_RANGE`].
pub fn kelvin_to_rgb(kelvin: u32) -> [f32; 3] {
    if kelvin == KELVIN_NEUTRAL {
        return [1.0, 1.0, 1.0];
    }
    let k = kelvin.clamp(KELVIN_RANGE.0, KELVIN_RANGE.1) as f64;
    let c = blackbody_linear(k);
    let w = blackbody_linear(KELVIN_NEUTRAL as f64);
    let rel = [c[0] / w[0], c[1] / w[1], c[2] / w[2]];
    let max = rel.iter().cloned().fold(f64::MIN, f64::max);
    let mut out = [1.0f32; 3];
    for (o, r) in out.iter_mut().zip(rel) {
        let lin = (r / max).clamp(0.0, 1.0);
        *o = lin.powf(1.0 / 2.2) as f32;
    }
    out
}

/// A ramp for a colour temperature and a brightness factor (1.0 = full,
/// limited to 0..=1). 6500 K at 1.0 is exactly the identity ramp.
pub fn ramp(kelvin: u32, factor: f32) -> GammaRamp {
    let rgb = kelvin_to_rgb(kelvin);
    let f: f64 = if factor.is_finite() {
        factor.clamp(0.0, 1.0) as f64
    } else {
        1.0
    };
    let mut out = GammaRamp::identity();
    for (ch, m) in [&mut out.r, &mut out.g, &mut out.b].into_iter().zip(rgb) {
        let m = m as f64 * f;
        for (i, v) in ch.iter_mut().enumerate() {
            let id = (i as u32 * 257) as f64;
            *v = (id * m).round().clamp(0.0, 65535.0) as u16;
        }
    }
    out
}

/// How far below the identity ramp an entry may go before Windows refuses
/// the whole ramp.
///
/// Assumption (documented, not from Microsoft): with the default
/// `GdiIcmGammaRange` setting, `SetDeviceGammaRamp` rejects ramps in which an
/// entry differs from the identity value (`i * 257`) by more than half the
/// range (32768), and ramps that are not ascending. We keep a margin below
/// that limit. Dimming or warming beyond it then simply stops at the limit
/// (about 50 % at the top of the ramp) instead of failing.
pub const WINDOWS_MAX_DEVIATION: u16 = 0x7E00;

/// Limits a ramp to what `SetDeviceGammaRamp` accepts by default: every entry
/// within [`WINDOWS_MAX_DEVIATION`] of the identity, and ascending.
pub fn clamp_for_windows(ramp: &GammaRamp) -> GammaRamp {
    let mut out = ramp.clone();
    for ch in [&mut out.r, &mut out.g, &mut out.b] {
        let mut prev = 0u16;
        for (i, v) in ch.iter_mut().enumerate() {
            let id = i as u32 * 257;
            let lo = id.saturating_sub(WINDOWS_MAX_DEVIATION as u32);
            let hi = (id + WINDOWS_MAX_DEVIATION as u32).min(65535);
            let clamped = (*v as u32).clamp(lo, hi) as u16;
            *v = clamped.max(prev);
            prev = *v;
        }
    }
    out
}

/// [`ramp`] limited with [`clamp_for_windows`].
pub fn ramp_safe(kelvin: u32, factor: f32) -> GammaRamp {
    clamp_for_windows(&ramp(kelvin, factor))
}

/// True when a ramp is within the limits of [`clamp_for_windows`].
pub fn within_windows_limits(ramp: &GammaRamp) -> bool {
    clamp_for_windows(ramp) == *ramp
}

/// Splits a user brightness (may be below zero) into the hardware
/// percentage and the software factor (1.0 = no software dimming):
/// 0..=100 is hardware only; below zero the hardware is at 0 and the screen
/// is darkened in software, down to `MIN_BELOW_ZERO` (-50 -> factor 0.5).
pub fn split_brightness(value: i32) -> (u32, f32) {
    if value >= 0 {
        (value.min(100) as u32, 1.0)
    } else {
        let v = value.max(MIN_BELOW_ZERO);
        (0, 1.0 + v as f32 / 100.0)
    }
}

/// Overlay opacity (0..=255) that darkens the screen to a software factor.
pub fn overlay_alpha(factor: f32) -> u8 {
    let f = if factor.is_finite() {
        factor.clamp(0.0, 1.0)
    } else {
        1.0
    };
    ((1.0 - f) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_white_and_identity() {
        assert_eq!(kelvin_to_rgb(6500), [1.0, 1.0, 1.0]);
        assert!(ramp(6500, 1.0).is_identity());
        assert!(ramp_safe(6500, 1.0).is_identity());
        // Close to 6500 the colour is close to white (smooth).
        let c = kelvin_to_rgb(6490);
        assert!(c.iter().all(|v| (v - 1.0).abs() < 0.01), "{c:?}");
        let c = kelvin_to_rgb(6510);
        assert!(c.iter().all(|v| (v - 1.0).abs() < 0.01), "{c:?}");
    }

    #[test]
    fn warmer_means_less_blue_and_green() {
        let mut prev = [1.0f32; 3];
        for k in (1000..=6500).rev().step_by(50) {
            let c = kelvin_to_rgb(k);
            assert_eq!(c[0], 1.0, "red at {k}");
            assert!(c[1] <= prev[1] + 1e-6, "green at {k}: {c:?} {prev:?}");
            assert!(c[2] <= prev[2] + 1e-6, "blue at {k}: {c:?} {prev:?}");
            assert!(c[2] <= c[1], "blue below green at {k}");
            prev = c;
        }
        // Familiar values (Redshift-like): 3400 K is clearly orange, not red.
        let c = kelvin_to_rgb(3400);
        assert!(c[1] > 0.7 && c[1] < 0.9, "{c:?}");
        assert!(c[2] > 0.45 && c[2] < 0.75, "{c:?}");
        let c = kelvin_to_rgb(1900);
        assert!(c[2] < 0.3, "{c:?}");
        // Above neutral it turns blue.
        let c = kelvin_to_rgb(9000);
        assert_eq!(c[2], 1.0);
        assert!(c[0] < 1.0);
    }

    #[test]
    fn ramps_are_ascending_and_scaled() {
        for k in [1900, 3400, 4500, 6500, 9000] {
            for f in [1.0, 0.7, 0.3] {
                let r = ramp(k, f);
                for ch in [&r.r, &r.g, &r.b] {
                    assert_eq!(ch[0], 0);
                    assert!(ch.windows(2).all(|w| w[0] <= w[1]));
                }
            }
        }
        let half = ramp(6500, 0.5);
        assert_eq!(half.r[255], 32768);
        assert_eq!(half.g[128], ((128 * 257) as f64 * 0.5).round() as u16);
        assert_eq!(ramp(6500, 0.0).b[255], 0);
        assert!(ramp(6500, 7.0).is_identity());
        assert!(ramp(6500, f32::NAN).is_identity());
        // Warm: blue reduced more than green, red untouched.
        let w = ramp(3400, 1.0);
        assert_eq!(w.r[255], 65535);
        assert!(w.b[255] < w.g[255] && w.g[255] < w.r[255]);
    }

    #[test]
    fn clamping_for_windows() {
        let deep = ramp(1900, 0.2);
        assert!(!within_windows_limits(&deep));
        let c = clamp_for_windows(&deep);
        assert!(within_windows_limits(&c));
        for ch in [&c.r, &c.g, &c.b] {
            for (i, &v) in ch.iter().enumerate() {
                let id = i as i64 * 257;
                assert!((v as i64 - id).abs() <= WINDOWS_MAX_DEVIATION as i64);
            }
            assert!(ch.windows(2).all(|w| w[0] <= w[1]));
        }
        // Mild ramps pass unchanged.
        let mild = ramp(5000, 0.9);
        assert_eq!(clamp_for_windows(&mild), mild);
        assert!(clamp_for_windows(&GammaRamp::identity()).is_identity());
        assert_eq!(ramp_safe(1900, 0.2), c);
    }

    #[test]
    fn brightness_split() {
        assert_eq!(split_brightness(70), (70, 1.0));
        assert_eq!(split_brightness(0), (0, 1.0));
        assert_eq!(split_brightness(140), (100, 1.0));
        assert_eq!(split_brightness(-50), (0, 0.5));
        assert_eq!(split_brightness(-20), (0, 0.8));
        assert_eq!(split_brightness(-90), (0, 0.5));
    }

    #[test]
    fn overlay_opacity() {
        assert_eq!(overlay_alpha(1.0), 0);
        assert_eq!(overlay_alpha(0.5), 128);
        assert_eq!(overlay_alpha(0.0), 255);
        assert_eq!(overlay_alpha(2.0), 0);
        assert!(overlay_alpha(0.8) < overlay_alpha(0.6));
    }
}
