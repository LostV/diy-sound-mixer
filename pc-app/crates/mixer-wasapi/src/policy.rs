//! Per-app output device, the same way Windows' own "App volume and device
//! preferences" page does it.
//!
//! There is no public API for this. Windows Settings, EarTrumpet and
//! SoundSwitch all use the internal `IAudioPolicyConfigFactory` WinRT factory.
//! Its interface id changed once (Windows 10 21H1); we ask for both known
//! ids. If Windows ever changes it again, `QueryInterface` simply fails, the
//! picker reports routing as unavailable, and nothing else is affected: we
//! never call into an interface we did not recognise by id.
//!
//! Windows stores the choice per executable and remembers it across restarts.

use std::ffi::c_void;

use windows::core::{GUID, HRESULT, HSTRING};

const CLASS: &str = "Windows.Media.Internal.AudioPolicyConfig";

/// Windows 10 21H2 and later, then the older downlevel variant. Same layout.
const IIDS: [GUID; 2] = [
    GUID::from_u128(0xab3d4648_e242_459f_b02f_541c70306324),
    GUID::from_u128(0x2a59116d_6c4f_45e0_a74f_707e3fef9258),
];

/// Vtable slots: 6 from IInspectable, then 19 methods we don't use.
const SLOT_SET_PERSISTED: usize = 25;
const SLOT_GET_PERSISTED: usize = 26;

/// `EDataFlow::eRender`, and the two roles Windows Settings sets together.
const RENDER: i32 = 0;
const ROLES: [i32; 2] = [0 /* eConsole */, 1 /* eMultimedia */];

/// Endpoint ids from `IMMDevice::GetId` must be wrapped in this device
/// interface path before the policy factory accepts them.
const MMDEVAPI_PREFIX: &str = r"\\?\SWD#MMDEVAPI#";
const RENDER_INTERFACE_SUFFIX: &str = "#{e6327cad-dcec-4949-ae8a-991e976a79d2}";

type SetPersisted = unsafe extern "system" fn(*mut c_void, u32, i32, i32, *mut c_void) -> HRESULT;
type GetPersisted =
    unsafe extern "system" fn(*mut c_void, u32, i32, i32, *mut *mut c_void) -> HRESULT;
type Release = unsafe extern "system" fn(*mut c_void) -> u32;

// Same raw-dylib linking the `windows` crate uses, so no SDK import library is needed.
windows_link::link!("combase.dll" "system" fn RoGetActivationFactory(class_id: *mut c_void, iid: *const GUID, factory: *mut *mut c_void) -> HRESULT);

/// Owned pointer to the factory, released on drop.
pub struct PolicyConfig(*mut c_void);

impl PolicyConfig {
    /// `None` if this Windows doesn't offer a factory we recognise.
    pub fn new() -> Option<Self> {
        let class = HSTRING::from(CLASS);
        for iid in &IIDS {
            let mut factory = std::ptr::null_mut();
            let hr = unsafe { RoGetActivationFactory(hstring_abi(&class), iid, &mut factory) };
            if hr.is_ok() && !factory.is_null() {
                return Some(Self(factory));
            }
        }
        None
    }

    /// Sends `pid`'s output to the endpoint `device_id` (as returned by
    /// `IMMDevice::GetId`), or back to the Windows default with `None`.
    pub fn set_app_output(&self, pid: u32, device_id: Option<&str>) -> windows::core::Result<()> {
        let path = device_id
            .map(|id| HSTRING::from(format!("{MMDEVAPI_PREFIX}{id}{RENDER_INTERFACE_SUFFIX}")));
        let raw = path.as_ref().map_or(std::ptr::null_mut(), hstring_abi);
        let set: SetPersisted = unsafe { std::mem::transmute(self.slot(SLOT_SET_PERSISTED)) };
        for role in ROLES {
            unsafe { set(self.0, pid, RENDER, role, raw) }.ok()?;
        }
        Ok(())
    }

    /// The endpoint path Windows has stored for `pid`, if any.
    pub fn app_output(&self, pid: u32) -> windows::core::Result<Option<String>> {
        let get: GetPersisted = unsafe { std::mem::transmute(self.slot(SLOT_GET_PERSISTED)) };
        let mut raw = std::ptr::null_mut();
        unsafe { get(self.0, pid, RENDER, ROLES[1], &mut raw) }.ok()?;
        // Take ownership so the string is freed.
        let value: HSTRING = unsafe { std::mem::transmute(raw) };
        Ok((!value.is_empty()).then(|| value.to_string()))
    }

    fn slot(&self, index: usize) -> *const c_void {
        unsafe {
            let vtable = *(self.0 as *const *const *const c_void);
            *vtable.add(index)
        }
    }
}

impl Drop for PolicyConfig {
    fn drop(&mut self) {
        unsafe {
            let release: Release = std::mem::transmute(self.slot(2));
            release(self.0);
        }
    }
}

/// The raw `HSTRING` handle, borrowed.
fn hstring_abi(value: &HSTRING) -> *mut c_void {
    unsafe { std::mem::transmute_copy(value) }
}
