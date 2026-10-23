//! Windows output filtering owned by a dynamic WFP session, independent of
//! Xray/TUN lifetime. Filters survive a core restart and never edit other rules.
#[cfg(windows)]
mod native {
    use anyhow::{Context, Result};
    use std::{net::IpAddr, ptr, sync::Mutex};
    use windows_sys::{
        core::GUID,
        Win32::{
            Foundation::HANDLE,
            NetworkManagement::{
                IpHelper::{ConvertInterfaceAliasToLuid, ConvertInterfaceLuidToIndex},
                Ndis::NET_LUID_LH,
                WindowsFilteringPlatform::*,
            },
        },
    };
    static ACTIVE: Mutex<Option<Guard>> = Mutex::new(None);
    struct Guard {
        handle: HANDLE,
        layer: GUID,
        ids: Vec<u64>,
    }
    // WFP handles support synchronized cross-thread calls. All access is mutex protected.
    unsafe impl Send for Guard {}
    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe {
                FwpmEngineClose0(self.handle);
            }
        }
    }
    fn check(code: u32) -> Result<()> {
        anyhow::ensure!(code == 0, "WFP error 0x{code:08x}");
        Ok(())
    }
    fn condition(
        key: GUID,
        kind: FWP_DATA_TYPE,
        data: FWP_CONDITION_VALUE0_0,
    ) -> FWPM_FILTER_CONDITION0 {
        FWPM_FILTER_CONDITION0 {
            fieldKey: key,
            matchType: FWP_MATCH_EQUAL,
            conditionValue: FWP_CONDITION_VALUE0 {
                r#type: kind,
                Anonymous: data,
            },
        }
    }
    fn number(key: GUID, value: u32) -> FWPM_FILTER_CONDITION0 {
        condition(key, FWP_UINT32, FWP_CONDITION_VALUE0_0 { uint32: value })
    }
    fn port(key: GUID, value: u16) -> FWPM_FILTER_CONDITION0 {
        condition(key, FWP_UINT16, FWP_CONDITION_VALUE0_0 { uint16: value })
    }
    fn protocol(value: u8) -> FWPM_FILTER_CONDITION0 {
        condition(
            FWPM_CONDITION_IP_PROTOCOL,
            FWP_UINT8,
            FWP_CONDITION_VALUE0_0 { uint8: value },
        )
    }
    impl Guard {
        fn new() -> Result<Self> {
            let mut handle = ptr::null_mut();
            let session = FWPM_SESSION0 {
                flags: FWPM_SESSION_FLAG_DYNAMIC,
                txnWaitTimeoutInMSec: 5000,
                ..Default::default()
            };
            unsafe {
                check(FwpmEngineOpen0(
                    ptr::null(),
                    10,
                    ptr::null(),
                    &session,
                    &mut handle,
                ))?;
            }
            let guard = Self {
                handle,
                layer: GUID::from_u128(uuid::Uuid::new_v4().as_u128()),
                ids: Vec::new(),
            };
            let mut name: Vec<u16> = "SNI Tunnel traffic protection\0".encode_utf16().collect();
            let layer = FWPM_SUBLAYER0 {
                subLayerKey: guard.layer,
                weight: 0x100,
                displayData: FWPM_DISPLAY_DATA0 {
                    name: name.as_mut_ptr(),
                    description: ptr::null_mut(),
                },
                ..Default::default()
            };
            unsafe {
                check(FwpmSubLayerAdd0(handle, &layer, ptr::null_mut()))?;
            }
            Ok(guard)
        }
        fn add(
            &self,
            layer: GUID,
            allow: bool,
            conditions: &mut [FWPM_FILTER_CONDITION0],
            ids: &mut Vec<u64>,
        ) -> Result<()> {
            let mut weight: u64 = if allow { 100 } else { 1 };
            let mut name: Vec<u16> = "SNI Tunnel owned output filter\0".encode_utf16().collect();
            let filter = FWPM_FILTER0 {
                layerKey: layer,
                subLayerKey: self.layer,
                displayData: FWPM_DISPLAY_DATA0 {
                    name: name.as_mut_ptr(),
                    description: ptr::null_mut(),
                },
                weight: FWP_VALUE0 {
                    r#type: FWP_UINT64,
                    Anonymous: FWP_VALUE0_0 {
                        uint64: &mut weight,
                    },
                },
                action: FWPM_ACTION0 {
                    r#type: if allow {
                        FWP_ACTION_PERMIT
                    } else {
                        FWP_ACTION_BLOCK
                    },
                    ..Default::default()
                },
                numFilterConditions: conditions.len() as u32,
                filterCondition: conditions.as_mut_ptr(),
                ..Default::default()
            };
            let mut id = 0;
            unsafe {
                check(FwpmFilterAdd0(
                    self.handle,
                    &filter,
                    ptr::null_mut(),
                    &mut id,
                ))?;
            }
            ids.push(id);
            Ok(())
        }
        fn configure(
            &mut self,
            index: u32,
            endpoints: &[String],
            endpoint_port: u16,
        ) -> Result<()> {
            unsafe {
                check(FwpmTransactionBegin0(self.handle, 0))?;
            }
            let mut ids = Vec::new();
            let result = (|| -> Result<()> {
                for id in &self.ids {
                    unsafe {
                        check(FwpmFilterDeleteById0(self.handle, *id))?;
                    }
                }
                for layer in [
                    FWPM_LAYER_OUTBOUND_TRANSPORT_V4,
                    FWPM_LAYER_OUTBOUND_TRANSPORT_V6,
                ] {
                    self.add(layer, false, &mut [], &mut ids)?;
                    let mut loopback = number(FWPM_CONDITION_FLAGS, FWP_CONDITION_FLAG_IS_LOOPBACK);
                    loopback.matchType = FWP_MATCH_FLAGS_ALL_SET;
                    self.add(layer, true, &mut [loopback], &mut ids)?;
                }
                // Only IPv4 is carried by the supported TUN datapath.
                self.add(
                    FWPM_LAYER_OUTBOUND_TRANSPORT_V4,
                    true,
                    &mut [number(FWPM_CONDITION_INTERFACE_INDEX, index)],
                    &mut ids,
                )?;
                for address in endpoints {
                    let ip: IpAddr = address
                        .parse()
                        .context("invalid resolved endpoint address")?;
                    let mut bytes = FWP_BYTE_ARRAY16::default();
                    let (layer, remote) = match ip {
                        IpAddr::V4(ip) => (
                            FWPM_LAYER_OUTBOUND_TRANSPORT_V4,
                            number(FWPM_CONDITION_IP_REMOTE_ADDRESS, u32::from(ip)),
                        ),
                        IpAddr::V6(ip) => {
                            bytes.byteArray16 = ip.octets();
                            (
                                FWPM_LAYER_OUTBOUND_TRANSPORT_V6,
                                condition(
                                    FWPM_CONDITION_IP_REMOTE_ADDRESS,
                                    FWP_BYTE_ARRAY16_TYPE,
                                    FWP_CONDITION_VALUE0_0 {
                                        byteArray16: &mut bytes,
                                    },
                                ),
                            )
                        }
                    };
                    self.add(
                        layer,
                        true,
                        &mut [
                            remote,
                            protocol(6),
                            port(FWPM_CONDITION_IP_REMOTE_PORT, endpoint_port),
                        ],
                        &mut ids,
                    )?;
                }
                // DHCP clients must retain access during Wi-Fi changes.
                for (layer, local, remote) in [
                    (FWPM_LAYER_OUTBOUND_TRANSPORT_V4, 68, 67),
                    (FWPM_LAYER_OUTBOUND_TRANSPORT_V6, 546, 547),
                ] {
                    self.add(
                        layer,
                        true,
                        &mut [
                            protocol(17),
                            port(FWPM_CONDITION_IP_LOCAL_PORT, local),
                            port(FWPM_CONDITION_IP_REMOTE_PORT, remote),
                        ],
                        &mut ids,
                    )?;
                }
                // Neighbor discovery/router solicitation: ICMP type is represented by local port.
                for kind in [133, 135, 136] {
                    self.add(
                        FWPM_LAYER_OUTBOUND_TRANSPORT_V6,
                        true,
                        &mut [protocol(58), port(FWPM_CONDITION_IP_LOCAL_PORT, kind)],
                        &mut ids,
                    )?;
                }
                unsafe {
                    check(FwpmTransactionCommit0(self.handle))?;
                }
                Ok(())
            })();
            if result.is_err() {
                unsafe {
                    FwpmTransactionAbort0(self.handle);
                }
            } else {
                self.ids = ids;
            }
            result
        }
    }
    pub fn install(device: &str, endpoints: &[String], port: u16) -> Result<()> {
        anyhow::ensure!(
            !endpoints.is_empty(),
            "Kill switch requires a resolved endpoint"
        );
        let name: Vec<u16> = device.encode_utf16().chain(Some(0)).collect();
        let mut luid = NET_LUID_LH::default();
        let mut index = 0;
        unsafe {
            check(ConvertInterfaceAliasToLuid(name.as_ptr(), &mut luid))?;
            check(ConvertInterfaceLuidToIndex(&luid, &mut index))?;
        }
        let mut active = ACTIVE.lock().unwrap();
        if active.is_none() {
            *active = Some(Guard::new()?);
        }
        active.as_mut().unwrap().configure(index, endpoints, port)
    }
    pub fn verify() -> Result<bool> {
        let active = ACTIVE.lock().unwrap();
        let Some(guard) = active.as_ref() else {
            return Ok(false);
        };
        anyhow::ensure!(!guard.ids.is_empty(), "Kill switch has no filters");
        for id in &guard.ids {
            let mut filter = ptr::null_mut();
            unsafe {
                check(FwpmFilterGetById0(guard.handle, *id, &mut filter))?;
                FwpmFreeMemory0(&mut filter as *mut _ as *mut *mut std::ffi::c_void);
            }
        }
        Ok(true)
    }
    pub fn clear() {
        ACTIVE.lock().unwrap().take();
    }
}
#[cfg(windows)]
pub use native::*;
#[cfg(not(windows))]
pub fn verify() -> anyhow::Result<bool> {
    Ok(false)
}
#[cfg(not(windows))]
pub fn clear() {}

#[cfg(all(test, windows))]
mod live_tests {
    use super::*;
    use std::{net::TcpStream, time::Duration};
    /// Deliberately restricted to ephemeral CI runners: temporarily blocks
    /// outbound traffic, with dynamic WFP cleanup on return, panic or exit.
    #[test]
    #[ignore = "requires an elevated disposable Windows runner"]
    fn live_wfp_blocks_fallback_and_releases_cleanly() {
        assert_eq!(
            std::env::var("SNI_WFP_DISPOSABLE_RUNNER").as_deref(),
            Ok("1")
        );
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                clear();
            }
        }
        let _cleanup = Cleanup;
        let allowed = "1.1.1.1:443".parse().unwrap();
        let blocked = "1.0.0.1:443".parse().unwrap();
        let timeout = Duration::from_secs(3);
        TcpStream::connect_timeout(&allowed, timeout).expect("baseline endpoint unavailable");
        TcpStream::connect_timeout(&blocked, timeout).expect("baseline test service unavailable");
        install("Loopback Pseudo-Interface 1", &["1.1.1.1".into()], 443).unwrap();
        assert!(verify().unwrap());
        TcpStream::connect_timeout(&allowed, timeout)
            .expect("guard blocked its endpoint exception");
        assert!(
            TcpStream::connect_timeout(&blocked, timeout).is_err(),
            "physical traffic bypassed WFP"
        );
        install("Loopback Pseudo-Interface 1", &["1.1.1.1".into()], 443).unwrap();
        assert!(verify().unwrap());
        assert!(
            TcpStream::connect_timeout(&blocked, timeout).is_err(),
            "replacement lost protection"
        );
        clear();
        TcpStream::connect_timeout(&blocked, timeout).expect("cleanup did not restore networking");
    }
}
