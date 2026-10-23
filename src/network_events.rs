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

#[cfg(windows)]
pub fn physical_available() -> bool {
    use windows_sys::Win32::NetworkManagement::IpHelper::*;
    // u64 provides the alignment required by the adapter structures.
    let mut size = 16384u32;
    for _ in 0..3 {
        let mut storage = vec![0u64; (size as usize).div_ceil(8)];
        let head = storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let status = unsafe {
            GetAdaptersAddresses(
                0,
                GAA_FLAG_INCLUDE_GATEWAYS,
                std::ptr::null(),
                head,
                &mut size,
            )
        };
        if status == 111 {
            continue;
        } // ERROR_BUFFER_OVERFLOW: adapter list changed.
        if status != 0 {
            return true;
        } // Unknown is not evidence of offline state.
        let mut current = head;
        while !current.is_null() {
            let adapter = unsafe { &*current };
            // The app installs split routes, not a default gateway on its TUN.
            // A working non-loopback adapter with a gateway is the uplink signal.
            if adapter.OperStatus == 1
                && adapter.IfType != 24
                && !adapter.FirstGatewayAddress.is_null()
            {
                return true;
            }
            current = adapter.Next;
        }
        return false;
    }
    true
}
