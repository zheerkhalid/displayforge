//! Gamma ramps per display device.
use crate::model::{GammaRamp, HwError};
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{CreateDCW, DeleteDC, HDC};
use windows::Win32::UI::ColorSystem::{GetDeviceGammaRamp, SetDeviceGammaRamp};

/// The memory layout SetDeviceGammaRamp expects: 3 x 256 WORDs.
#[repr(C)]
struct RawRamp([[u16; 256]; 3]);

impl RawRamp {
    fn from_ramp(r: &GammaRamp) -> Self {
        RawRamp([r.r, r.g, r.b])
    }
    fn into_ramp(self) -> GammaRamp {
        GammaRamp {
            r: self.0[0],
            g: self.0[1],
            b: self.0[2],
        }
    }
}

/// A device context for one display, deleted on drop.
struct DisplayDc(HDC);

impl DisplayDc {
    fn open(device: &str) -> Result<Self, HwError> {
        let wide = super::wide(device);
        // SAFETY: `wide` is NUL-terminated and outlives the call.
        let dc = unsafe { CreateDCW(PCWSTR::null(), PCWSTR(wide.as_ptr()), PCWSTR::null(), None) };
        if dc.is_invalid() {
            Err(HwError(
                "Could not open this display to change its colours. It may have been disconnected."
                    .into(),
            ))
        } else {
            Ok(DisplayDc(dc))
        }
    }
}

impl Drop for DisplayDc {
    fn drop(&mut self) {
        // SAFETY: the DC was created by CreateDCW and is deleted once.
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

/// Sets the gamma ramp of a GDI display device (`\\.\DISPLAY1`); `None` restores identity.
pub fn set_device_gamma(device: &str, ramp: Option<&GammaRamp>) -> Result<(), HwError> {
    let identity = GammaRamp::identity();
    let raw = RawRamp::from_ramp(ramp.unwrap_or(&identity));
    let dc = DisplayDc::open(device)?;
    // SAFETY: `raw` has the 3 x 256 WORD layout the API reads; the DC is valid.
    let ok = unsafe { SetDeviceGammaRamp(dc.0, &raw as *const RawRamp as *const _) };
    if ok.as_bool() {
        Ok(())
    } else if ramp.is_some_and(|r| !r.is_identity()) {
        Err(HwError(
            "Windows refused this colour change: it is too strong for this display. \
             Try a milder night light or less software dimming."
                .into(),
        ))
    } else {
        Err(HwError(
            "Windows refused to reset this display's colours. The graphics driver may not support colour ramps."
                .into(),
        ))
    }
}

/// Reads the current gamma ramp of a GDI display device.
pub fn get_device_gamma(device: &str) -> Result<GammaRamp, HwError> {
    let dc = DisplayDc::open(device)?;
    let mut raw = RawRamp([[0; 256]; 3]);
    // SAFETY: `raw` is a writable 3 x 256 WORD buffer; the DC is valid.
    let ok = unsafe { GetDeviceGammaRamp(dc.0, &mut raw as *mut RawRamp as *mut _) };
    if ok.as_bool() {
        Ok(raw.into_ramp())
    } else {
        Err(HwError(
            "Could not read this display's colour ramp. The graphics driver may not support it."
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_round_trip() {
        assert_eq!(std::mem::size_of::<RawRamp>(), 3 * 256 * 2);
        let mut r = GammaRamp::identity();
        r.g[10] = 7;
        r.b[255] = 1000;
        assert_eq!(RawRamp::from_ramp(&r).into_ramp(), r);
    }
}
