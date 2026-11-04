//! Read-only guard against the pinned DLL's shared-driver install/upgrade path.
use super::{status, wide, win};
use anyhow::{ensure, Context, Result};
use std::{mem::{size_of, zeroed}, ptr};
use windows_sys::{core::GUID, Win32::{Devices::DeviceAndDriverInstallation::*, Foundation::*}};

const NET_CLASS: GUID = GUID::from_u128(0x4d36e972_e325_11ce_bfc1_08002be10318);
// Pinned DLL's data-only wintun.inf resource: SHA256 0181e3af3e45efd36fa74de8245103c2c93ffc01e896dd4235f55d89e26d64b3.
const PINNED_DATE: u64 = 132785568000000000;
const PINNED_VERSION: u64 = 14 << 32;
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Package { pub date: u64, pub version: u64, pub provider: String }
struct DeviceSet(HDEVINFO);
impl Drop for DeviceSet { fn drop(&mut self) { unsafe { SetupDiDestroyDeviceInfoList(self.0); } } }

/// The device-info element is never registered and no class installer is called.
pub fn inventory() -> Result<Vec<Package>> {
    let set = DeviceSet(unsafe { SetupDiCreateDeviceInfoList(&NET_CLASS, ptr::null_mut()) });
    ensure!(set.0 != -1, "driver_inventory_device_list_failed");
    let mut device: SP_DEVINFO_DATA = unsafe { zeroed() }; device.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
    unsafe {
        win(SetupDiCreateDeviceInfoW(set.0, wide("Wintun")?.as_ptr(), &NET_CLASS, ptr::null(), ptr::null_mut(), DICD_GENERATE_ID, &mut device)).context("driver_inventory_create_unregistered_element")?;
        let hardware: Vec<u16> = "Wintun".encode_utf16().chain([0, 0]).collect();
        win(SetupDiSetDeviceRegistryPropertyW(set.0, &mut device, SPDRP_HARDWAREID, hardware.as_ptr().cast(), (hardware.len() * 2) as u32)).context("driver_inventory_set_element_hardware_id")?;
        win(SetupDiBuildDriverInfoList(set.0, &mut device, SPDIT_COMPATDRIVER)).context("driver_inventory_build_compatible_list")?;
    }
    let mut packages = vec![];
    for index in 0..256 {
        let mut row: SP_DRVINFO_DATA_V2_W = unsafe { zeroed() }; row.cbSize = size_of::<SP_DRVINFO_DATA_V2_W>() as u32;
        if unsafe { SetupDiEnumDriverInfoW(set.0, &device, SPDIT_COMPATDRIVER, index, &mut row) } == 0 {
            let code = unsafe { GetLastError() }; if code == ERROR_NO_MORE_ITEMS { return Ok(packages); }
            status(code).context("driver_inventory_enumerate")?;
        }
        let length = row.ProviderName.iter().position(|v| *v == 0).context("unterminated_driver_provider")?;
        packages.push(Package { date: (u64::from(row.DriverDate.dwHighDateTime) << 32) | u64::from(row.DriverDate.dwLowDateTime), version: row.DriverVersion, provider: String::from_utf16(&row.ProviderName[..length])? });
    }
    anyhow::bail!("driver_inventory_limit")
}
pub(super) fn validate(packages: &[Package]) -> Result<()> {
    ensure!(!packages.is_empty(), "preinstalled_wintun_driver_required");
    for package in packages {
        ensure!(package.provider == "WireGuard LLC", "unknown_compatible_wintun_driver_provider");
        // DriverInstall examines every candidate; an older one triggers shared-adapter disable even when a current one exists.
        ensure!((package.date, package.version) >= (PINNED_DATE, PINNED_VERSION), "older_compatible_wintun_driver_would_trigger_upgrade");
    }
    ensure!(packages.iter().any(|v| (v.date, v.version) == (PINNED_DATE, PINNED_VERSION)), "pinned_preinstalled_wintun_driver_required");
    Ok(())
}
pub fn require_preinstalled() -> Result<()> { validate(&inventory()?) }

#[cfg(test)]
mod tests {
    use super::*;
    fn pinned() -> Package { Package { date: PINNED_DATE, version: PINNED_VERSION, provider: "WireGuard LLC".into() } }
    #[test]
    fn every_candidate_is_checked_before_permitting_shared_driver_use() {
        let good = pinned(); validate(std::slice::from_ref(&good)).unwrap();
        assert!(validate(&[]).unwrap_err().to_string().contains("preinstalled"));
        for (date, version) in [(PINNED_DATE - 1, PINNED_VERSION + 1), (PINNED_DATE, PINNED_VERSION - 1)] {
            let old = Package { date, version, ..pinned() };
            assert!(validate(&[good.clone(), old.clone()]).is_err());
            assert!(validate(&[old, good.clone()]).is_err());
        }
        let newer = Package { date: PINNED_DATE + 1, version: 1, ..pinned() };
        assert!(validate(std::slice::from_ref(&newer)).is_err()); validate(&[newer, good.clone()]).unwrap();
        assert!(validate(&[Package { provider: "other".into(), ..good }]).is_err());
    }
}
