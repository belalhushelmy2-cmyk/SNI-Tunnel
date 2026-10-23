//! Coalesced native network-change signals; no callback does networking work.
use std::sync::atomic::{AtomicBool, Ordering};
static CHANGED: AtomicBool = AtomicBool::new(false);
pub fn take_changed() -> bool {
    CHANGED.swap(false, Ordering::AcqRel)
}
#[cfg(windows)]
pub struct Watch {
    interface: windows_sys::Win32::Foundation::HANDLE,
    routes: windows_sys::Win32::Foundation::HANDLE,
}
#[cfg(windows)]
unsafe impl Send for Watch {}
#[cfg(windows)]
impl Watch {
    pub fn new() -> anyhow::Result<Self> {
        use windows_sys::Win32::NetworkManagement::IpHelper::*;
        unsafe extern "system" fn interface(
            _: *const std::ffi::c_void,
            _: *const MIB_IPINTERFACE_ROW,
            _: MIB_NOTIFICATION_TYPE,
        ) {
            CHANGED.store(true, Ordering::Release);
        }
        unsafe extern "system" fn route(
            _: *const std::ffi::c_void,
            _: *const MIB_IPFORWARD_ROW2,
            _: MIB_NOTIFICATION_TYPE,
        ) {
            CHANGED.store(true, Ordering::Release);
        }
        let mut watch = Self {
            interface: std::ptr::null_mut(),
            routes: std::ptr::null_mut(),
        };
        unsafe {
            anyhow::ensure!(
                NotifyIpInterfaceChange(
                    0,
                    Some(interface),
                    std::ptr::null(),
                    false,
                    &mut watch.interface
                ) == 0,
                "Interface notifications unavailable"
            );
            anyhow::ensure!(
                NotifyRouteChange2(0, Some(route), std::ptr::null(), false, &mut watch.routes) == 0,
                "Route notifications unavailable"
            );
        }
        Ok(watch)
    }
}
#[cfg(windows)]
impl Drop for Watch {
    fn drop(&mut self) {
        unsafe {
            use windows_sys::Win32::NetworkManagement::IpHelper::CancelMibChangeNotify2;
            if !self.interface.is_null() {
                CancelMibChangeNotify2(self.interface);
            }
            if !self.routes.is_null() {
                CancelMibChangeNotify2(self.routes);
            }
        }
    }
}
