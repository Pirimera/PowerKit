//! Read-only discovery of connected HID collections for device adapters.
//! Nothing is written to a device: each collection is opened without read or
//! write access, only to ask Windows for its ids, usage and product string.
#![expect(
    unsafe_code,
    reason = "CfgMgr32 interface list and HidD/HidP attribute queries"
)]
use powerkit_core::power_mouse::device::HidInterface;
use windows_sys::Win32::{
    Devices::{
        DeviceAndDriverInstallation::{
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
            CM_Get_Device_Interface_ListW, CR_BUFFER_SMALL, CR_SUCCESS,
        },
        HumanInterfaceDevice::{
            HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS, HidD_FreePreparsedData,
            HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData, HidD_GetProductString,
            HidP_GetCaps,
        },
    },
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
};

/// Every present HID collection that could be queried. A collection that
/// cannot be opened or described is skipped; discovery never fails as a whole.
pub fn enumerate() -> Vec<HidInterface> {
    interface_paths()
        .into_iter()
        .filter_map(|path| describe(&path))
        .collect()
}

/// Device interface paths of the HID class, as null-terminated UTF-16.
fn interface_paths() -> Vec<Vec<u16>> {
    let mut class = windows_sys::core::GUID::default();
    // SAFETY: class is a writable GUID.
    unsafe {
        HidD_GetHidGuid(&raw mut class);
    }
    // The list can grow between the size query and the read; retry a few times.
    for _ in 0..4 {
        let mut len = 0u32;
        // SAFETY: len is writable; a null device id lists the whole class.
        let sized = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &raw mut len,
                &raw const class,
                std::ptr::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if sized != CR_SUCCESS || len == 0 {
            return Vec::new();
        }
        let mut buffer = vec![0u16; len as usize];
        // SAFETY: buffer holds len UTF-16 units and is writable.
        let listed = unsafe {
            CM_Get_Device_Interface_ListW(
                &raw const class,
                std::ptr::null(),
                buffer.as_mut_ptr(),
                len,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        match listed {
            CR_SUCCESS => return split_multi_sz(&buffer),
            CR_BUFFER_SMALL => continue,
            _ => return Vec::new(),
        }
    }
    Vec::new()
}

/// Splits a double-null-terminated list into strings that keep one null.
fn split_multi_sz(buffer: &[u16]) -> Vec<Vec<u16>> {
    buffer
        .split(|unit| *unit == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let mut owned = path.to_vec();
            owned.push(0);
            owned
        })
        .collect()
}

/// Closes the collection handle on every exit path.
struct Collection(HANDLE);

impl Drop for Collection {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateFileW and is closed once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn describe(path: &[u16]) -> Option<HidInterface> {
    // SAFETY: path is null-terminated. Zero desired access opens the
    // collection for attribute queries only, which also works for the mouse
    // and keyboard collections Windows holds exclusively.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return None;
    }
    let collection = Collection(handle);

    let mut attributes = HIDD_ATTRIBUTES {
        Size: u32::try_from(size_of::<HIDD_ATTRIBUTES>()).ok()?,
        ..HIDD_ATTRIBUTES::default()
    };
    // SAFETY: live handle; attributes is writable with its Size set.
    if !unsafe { HidD_GetAttributes(collection.0, &raw mut attributes) } {
        return None;
    }
    let (usage_page, usage) = usage(&collection)?;
    Some(HidInterface {
        path: String::from_utf16_lossy(&path[..path.len().saturating_sub(1)]),
        vendor_id: attributes.VendorID,
        product_id: attributes.ProductID,
        usage_page,
        usage,
        product: product(&collection),
    })
}

fn usage(collection: &Collection) -> Option<(u16, u16)> {
    let mut preparsed = 0;
    // SAFETY: live handle; preparsed receives data owned until freed below.
    if !unsafe { HidD_GetPreparsedData(collection.0, &raw mut preparsed) } {
        return None;
    }
    let mut caps = HIDP_CAPS::default();
    // SAFETY: preparsed was just returned; caps is writable.
    let status = unsafe { HidP_GetCaps(preparsed, &raw mut caps) };
    // SAFETY: preparsed is freed exactly once.
    unsafe {
        HidD_FreePreparsedData(preparsed);
    }
    (status == HIDP_STATUS_SUCCESS).then_some((caps.UsagePage, caps.Usage))
}

fn product(collection: &Collection) -> String {
    let mut buffer = [0u16; 128];
    // SAFETY: buffer is writable and its byte length is passed.
    let ok = unsafe {
        HidD_GetProductString(
            collection.0,
            buffer.as_mut_ptr().cast(),
            u32::try_from(size_of_val(&buffer)).unwrap_or(0),
        )
    };
    if !ok {
        return String::new();
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_sz_list_splits_into_terminated_paths() {
        let list: Vec<u16> = "a\0bc\0\0".encode_utf16().collect();
        let paths = split_multi_sz(&list);
        assert_eq!(paths, vec![vec![97, 0], vec![98, 99, 0]]);
        assert!(split_multi_sz(&[0]).is_empty());
    }

    /// Prints what this machine reports, for checking the classification
    /// against real hardware. Device addresses are masked in the output.
    #[test]
    #[ignore = "reads the HID devices connected to this machine"]
    fn probe_prints_logitech_collections() {
        let interfaces = enumerate();
        println!("{} HID collections", interfaces.len());
        for interface in &interfaces {
            if interface.vendor_id != powerkit_core::power_mouse::device::LOGITECH_VENDOR_ID {
                continue;
            }
            let hardware_id = interface.path.split('#').nth(1).unwrap_or_default();
            let masked: String = hardware_id
                .split('&')
                .map(|part| {
                    let is_address = part.len() == 12
                        || part.rsplit('_').next().is_some_and(|tail| {
                            tail.len() == 12 && tail.chars().all(|c| c.is_ascii_hexdigit())
                        });
                    if is_address { "<address>" } else { part }
                })
                .collect::<Vec<_>>()
                .join("&");
            println!(
                "pid={:04X} page={:04X} usage={:04X} hidpp={:?} product={:?} id={masked}",
                interface.product_id,
                interface.usage_page,
                interface.usage,
                interface.hidpp(),
                interface.product,
            );
        }
        for device in powerkit_core::power_mouse::device::devices(&interfaces) {
            println!(
                "device name={:?} pid={:04X} transport={:?} hidpp={:?}",
                device.name, device.product_id, device.transport, device.hidpp
            );
        }
    }
}
