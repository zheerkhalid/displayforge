//! Built-in panel brightness through WMI (WmiMonitorBrightness / WmiMonitorBrightnessMethods).
//!
//! WMI is reached through COM on the calling thread. The connection is kept
//! per thread (COM objects must stay on the thread that made them). Desktop
//! PCs have no such WMI classes: every function then fails quietly with a
//! plain error or an empty list.
use crate::model::HwError;
use std::cell::{Cell, RefCell};
use windows::core::{w, BSTR, PCWSTR, VARIANT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemContext, IWbemLocator, IWbemServices, WbemLocator,
    WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE,
};

/// One built-in panel as WMI lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panel {
    /// e.g. `DISPLAY\ABC1234\4&1a2b3c4d&0&UID1234_0`.
    pub instance: String,
    /// Current brightness in percent.
    pub brightness: u8,
    pub active: bool,
}

const NO_WMI: &str = "Windows does not offer brightness control for this screen.";

thread_local! {
    static COM_READY: Cell<bool> = const { Cell::new(false) };
    static SERVICES: RefCell<Option<IWbemServices>> = const { RefCell::new(None) };
}

fn ensure_com() {
    COM_READY.with(|ready| {
        if !ready.get() {
            // SAFETY: initialises COM for this thread. S_FALSE (already
            // initialised) and RPC_E_CHANGED_MODE (an STA thread) are both fine:
            // WMI works from either apartment.
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            ready.set(true);
        }
    });
}

fn connect() -> windows::core::Result<IWbemServices> {
    ensure_com();
    // SAFETY: plain COM calls; every argument outlives its call.
    unsafe {
        let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
        let services = locator.ConnectServer(
            &BSTR::from("ROOT\\WMI"),
            &BSTR::new(),
            &BSTR::new(),
            &BSTR::new(),
            0,
            &BSTR::new(),
            None::<&IWbemContext>,
        )?;
        CoSetProxyBlanket(
            &services,
            10, // RPC_C_AUTHN_WINNT
            0,  // RPC_C_AUTHZ_NONE
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )?;
        Ok(services)
    }
}

/// Runs `f` with this thread's WMI connection (made on first use). A failed
/// call drops the connection so that the next call reconnects.
fn with_services<T>(
    f: impl FnOnce(&IWbemServices) -> windows::core::Result<T>,
) -> windows::core::Result<T> {
    SERVICES.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(connect()?);
        }
        let result = f(slot.as_ref().expect("connected above"));
        if result.is_err() {
            *slot = None;
        }
        result
    })
}

fn query(services: &IWbemServices, wql: &str) -> windows::core::Result<Vec<IWbemClassObject>> {
    let mut out = Vec::new();
    // SAFETY: plain COM calls; the BSTRs outlive the call.
    unsafe {
        let rows = services.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from(wql),
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None::<&IWbemContext>,
        )?;
        loop {
            let mut row = [None];
            let mut returned = 0u32;
            rows.Next(WBEM_INFINITE, &mut row, &mut returned).ok()?;
            match row[0].take() {
                Some(obj) if returned > 0 => out.push(obj),
                _ => break,
            }
        }
    }
    Ok(out)
}

fn get_prop(obj: &IWbemClassObject, name: PCWSTR) -> windows::core::Result<VARIANT> {
    let mut v = VARIANT::default();
    // SAFETY: `name` is a static wide string; `v` is a valid VARIANT.
    unsafe { obj.Get(name, 0, &mut v, None, None)? };
    Ok(v)
}

fn get_string(obj: &IWbemClassObject, name: PCWSTR) -> String {
    get_prop(obj, name)
        .ok()
        .and_then(|v| BSTR::try_from(&v).ok())
        .map(|b| b.to_string())
        .unwrap_or_default()
}

/// The built-in panels whose brightness Windows can control. Empty on PCs
/// without one (and whenever WMI is not available).
pub fn panels() -> Vec<Panel> {
    with_services(|s| {
        let rows = query(
            s,
            "SELECT InstanceName, CurrentBrightness, Active FROM WmiMonitorBrightness",
        )?;
        Ok(rows
            .iter()
            .map(|o| Panel {
                instance: get_string(o, w!("InstanceName")),
                brightness: get_prop(o, w!("CurrentBrightness"))
                    .ok()
                    .and_then(|v| u32::try_from(&v).ok())
                    .unwrap_or(0)
                    .min(100) as u8,
                active: get_prop(o, w!("Active"))
                    .ok()
                    .and_then(|v| bool::try_from(&v).ok())
                    .unwrap_or(true),
            })
            .collect())
    })
    .unwrap_or_default()
}

/// The hardware instance id of a monitor (`DISPLAY\ABC1234\5&xxxx&0&UID4352`)
/// from its device interface path (`\\?\DISPLAY#ABC1234#5&xxxx&0&UID4352#{guid}`).
pub fn hardware_id(interface_path: &str) -> Option<String> {
    let rest = interface_path
        .strip_prefix("\\\\?\\")
        .unwrap_or(interface_path);
    let mut parts: Vec<&str> = rest.split('#').collect();
    if parts.len() < 3 {
        return None;
    }
    if parts.last().is_some_and(|p| p.starts_with('{')) {
        parts.pop();
    }
    Some(parts.join("\\"))
}

/// Which WMI instance belongs to a monitor with this hardware id: an exact
/// match (WMI adds `_0`), else the only active panel when there is just one.
pub fn pick_instance(instances: &[&str], hardware_id: Option<&str>) -> Option<usize> {
    if let Some(id) = hardware_id {
        let id = id.to_ascii_uppercase();
        let found = instances.iter().position(|inst| {
            let inst = inst.to_ascii_uppercase();
            inst == id || inst.strip_prefix(&id).is_some_and(|r| r.starts_with('_'))
        });
        if found.is_some() {
            return found;
        }
    }
    if instances.len() == 1 {
        Some(0)
    } else {
        None
    }
}

fn find_panel(hardware_id: Option<&str>) -> Result<Panel, HwError> {
    let panels: Vec<Panel> = panels().into_iter().filter(|p| p.active).collect();
    let names: Vec<&str> = panels.iter().map(|p| p.instance.as_str()).collect();
    pick_instance(&names, hardware_id)
        .map(|i| panels[i].clone())
        .ok_or_else(|| HwError(NO_WMI.into()))
}

/// Current brightness (percent) of the built-in panel with this hardware id.
pub fn get_brightness(hardware_id: Option<&str>) -> Result<u8, HwError> {
    find_panel(hardware_id).map(|p| p.brightness)
}

/// Whether Windows can control this built-in panel's brightness.
pub fn available(hardware_id: Option<&str>) -> bool {
    find_panel(hardware_id).is_ok()
}

/// Sets the brightness (percent) of the built-in panel with this hardware id.
pub fn set_brightness(hardware_id: Option<&str>, percent: u8) -> Result<(), HwError> {
    let panel = find_panel(hardware_id)?;
    let percent = percent.min(100);
    with_services(|s| {
        let methods = query(
            s,
            "SELECT InstanceName, __PATH FROM WmiMonitorBrightnessMethods",
        )?;
        let names: Vec<String> = methods
            .iter()
            .map(|o| get_string(o, w!("InstanceName")))
            .collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let index = pick_instance(&refs, Some(&panel.instance))
            .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
        let path = get_string(&methods[index], w!("__PATH"));
        // SAFETY: plain COM calls; every argument outlives its call.
        unsafe {
            let mut class = None;
            s.GetObject(
                &BSTR::from("WmiMonitorBrightnessMethods"),
                WBEM_GENERIC_FLAG_TYPE(0),
                None::<&IWbemContext>,
                Some(&mut class),
                None,
            )?;
            let class = class.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
            let mut signature = None;
            class.GetMethod(
                w!("WmiSetBrightness"),
                0,
                &mut signature,
                std::ptr::null_mut(),
            )?;
            let signature = signature.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
            let args = signature.SpawnInstance(0)?;
            args.Put(w!("Timeout"), 0, &VARIANT::from(0i32), 0)?;
            args.Put(w!("Brightness"), 0, &VARIANT::from(percent), 0)?;
            s.ExecMethod(
                &BSTR::from(path),
                &BSTR::from("WmiSetBrightness"),
                WBEM_GENERIC_FLAG_TYPE(0),
                None::<&IWbemContext>,
                &args,
                None,
                None,
            )?;
        }
        Ok(())
    })
    .map_err(|_| {
        HwError("Windows did not accept the new brightness for the built-in screen. Try again, or use the brightness keys.".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_id_from_path() {
        assert_eq!(
            hardware_id(
                "\\\\?\\DISPLAY#ABC1234#5&1f2e3d&0&UID4352#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}"
            )
            .as_deref(),
            Some("DISPLAY\\ABC1234\\5&1f2e3d&0&UID4352")
        );
        assert_eq!(hardware_id("garbage"), None);
    }

    #[test]
    fn instance_matching() {
        let list = [
            "DISPLAY\\AAA0001\\4&11&0&UID1_0",
            "DISPLAY\\BBB0002\\4&22&0&UID2_0",
        ];
        assert_eq!(
            pick_instance(&list, Some("display\\bbb0002\\4&22&0&uid2")),
            Some(1)
        );
        assert_eq!(pick_instance(&list, Some("DISPLAY\\CCC\\X")), None);
        // A prefix of another id must not match.
        assert_eq!(
            pick_instance(&list, Some("DISPLAY\\AAA0001\\4&11&0&UID")),
            None
        );
        // One panel only: take it.
        assert_eq!(pick_instance(&list[..1], Some("DISPLAY\\CCC\\X")), Some(0));
        assert_eq!(pick_instance(&list[..1], None), Some(0));
        assert_eq!(pick_instance(&[], None), None);
    }
}
