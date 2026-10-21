//! Implements webrtc-jni/src/main/cpp/include/api/CubebAudioDeviceModuleApi.h.

use std::{
    ffi::{CStr, c_char, c_void},
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::{anyhow, bail};

use crate::{
    core::util::truncate_for_logging,
    webrtc::{
        audio_device_module::{AudioDeviceModule, SYSTEM_DEFAULT_DEVICE_ID},
        audio_device_module_callbacks::{AUDIO_DEVICE_CBS_PTR, decrement_adm_ref_count},
        peer_connection_factory::{AudioDevice, AudioDeviceList, AudioDeviceObserver},
        ptr::Borrowed,
    },
};

pub type CubebAdm = Mutex<AudioDeviceModule>;

pub type CubebAdmDevicesChanged = extern "C" fn(context: *mut c_void);

const SUCCESS: i32 = 0;
const FAILURE: i32 = -1;

// Request IDs only tag Signal Desktop's promise results, which this API does not report.
const UNUSED_REQUEST_ID: u32 = 0;

// kAdmMaxDeviceNameSize and kAdmMaxGuidSize in WebRTC's audio_device_defines.h.
const ADM_MAX_DEVICE_NAME_SIZE: usize = 128;
const ADM_MAX_GUID_SIZE: usize = 128;

// FNV-1a 64-bit parameters, chosen because the hash must not change between builds.
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
const HASHED_DEVICE_ID_PREFIX: &str = "fnv1a:";

#[derive(Clone, Copy, Debug)]
enum Direction {
    Playout,
    Recording,
}

#[derive(Debug)]
struct DevicesChangedObserver {
    callback: Option<CubebAdmDevicesChanged>,
    context: *mut c_void,
}

// Safety: the C API requires a context that the cubeb worker thread may use.
unsafe impl Send for DevicesChangedObserver {}

impl DevicesChangedObserver {
    fn notify(&self) {
        if let Some(callback) = self.callback {
            callback(self.context);
        }
    }
}

impl AudioDeviceObserver for DevicesChangedObserver {
    fn output_changed(&self, _devices: AudioDeviceList) {
        self.notify();
    }

    fn input_changed(&self, _devices: AudioDeviceList) {
        self.notify();
    }

    fn settle_promise(&self, _success: bool, _audio_device: Option<AudioDevice>, _request_id: u32) {
    }

    fn settle_start_recording_via_callback(
        &self,
        _success: bool,
        _audio_device: Option<AudioDevice>,
    ) {
    }

    fn settle_start_playback_via_callback(
        &self,
        _success: bool,
        _audio_device: Option<AudioDevice>,
    ) {
    }
}

fn status(result: anyhow::Result<()>, operation: &str) -> i32 {
    match result {
        Ok(()) => SUCCESS,
        Err(e) => {
            error!("{operation} failed: {e:?}");
            FAILURE
        }
    }
}

/// # Safety
/// `adm` is NULL or a handle from `cubeb_adm_create` that has not been released.
unsafe fn lock<'a>(adm: *const CubebAdm) -> anyhow::Result<MutexGuard<'a, AudioDeviceModule>> {
    let adm = unsafe { adm.as_ref() }.ok_or_else(|| anyhow!("null handle"))?;
    adm.lock().map_err(|e| anyhow!("Mutex was poisoned? {e}"))
}

/// The ID WebRTC gets for a device: an ID that does not fit its buffer becomes a stable hash.
fn webrtc_device_id(unique_id: &str) -> String {
    if unique_id.len() < ADM_MAX_GUID_SIZE {
        return unique_id.to_string();
    }
    let hash = unique_id.bytes().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    });
    format!("{HASHED_DEVICE_ID_PREFIX}{hash:016x}")
}

fn list_devices(
    adm: &mut AudioDeviceModule,
    direction: Direction,
) -> anyhow::Result<AudioDeviceList> {
    match direction {
        Direction::Playout => adm.get_audio_playout_devices(),
        Direction::Recording => adm.get_audio_recording_devices(),
    }
}

/// WebRTC's device list as (name, ID): the system default first, then every device.
fn webrtc_devices(
    adm: &mut AudioDeviceModule,
    direction: Direction,
) -> anyhow::Result<Vec<(String, String)>> {
    let devices = list_devices(adm, direction)?;
    let default = devices.default.default_for_voip().map(|device| {
        (
            format!("default ({})", device.name),
            SYSTEM_DEFAULT_DEVICE_ID.to_string(),
        )
    });
    let listed = devices
        .list
        .iter()
        .map(|device| (device.name.clone(), webrtc_device_id(&device.unique_id)));
    Ok(default.into_iter().chain(listed).collect())
}

/// Copies `src` and a nul terminator into `size` bytes, cutting it at a character boundary.
///
/// # Safety
/// `dest` is NULL or valid for writing `size` bytes.
unsafe fn copy_c_string(src: &str, dest: *mut c_char, size: usize) -> anyhow::Result<()> {
    if dest.is_null() {
        bail!("null buffer");
    }
    let Some(max_len) = size.checked_sub(1) else {
        bail!("empty buffer");
    };
    let len = src.floor_char_boundary(max_len);
    unsafe {
        std::ptr::copy_nonoverlapping(src.as_ptr(), dest.cast::<u8>(), len);
        dest.add(len).write(0);
    }
    Ok(())
}

fn select_device(
    adm: &mut AudioDeviceModule,
    direction: Direction,
    id: &str,
) -> anyhow::Result<()> {
    let devices = list_devices(adm, direction)?;
    let id = if id == SYSTEM_DEFAULT_DEVICE_ID {
        id
    } else {
        &devices
            .list
            .iter()
            .find(|d| webrtc_device_id(&d.unique_id) == id)
            .ok_or_else(|| {
                anyhow!(
                    "No {direction:?} device with ID {}",
                    truncate_for_logging(id)
                )
            })?
            .unique_id
    };
    match direction {
        Direction::Playout => adm.set_playout_device_by_id(id, UNUSED_REQUEST_ID),
        Direction::Recording => adm.set_recording_device_by_id(id, UNUSED_REQUEST_ID),
    }
}

/// # Safety
/// `adm` as for [`lock`]; `id` is NULL or a nul-terminated string.
unsafe fn set_device(
    adm: *const CubebAdm,
    direction: Direction,
    id: *const c_char,
) -> anyhow::Result<()> {
    if id.is_null() {
        bail!("null device ID");
    }
    let id = unsafe { CStr::from_ptr(id) }.to_str()?;
    select_device(&mut *unsafe { lock(adm) }?, direction, id)
}

#[unsafe(no_mangle)]
pub extern "C" fn cubeb_adm_create() -> *const CubebAdm {
    let adm = match AudioDeviceModule::new() {
        Ok(adm) => adm,
        Err(e) => {
            error!("Failed to create the audio device module: {e:?}");
            return std::ptr::null();
        }
    };
    Arc::into_raw(Arc::new(Mutex::new(adm)))
}

/// # Safety
/// `adm` is NULL or a handle from `cubeb_adm_create` whose reference has not been released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cubeb_adm_release(adm: *const CubebAdm) {
    unsafe { decrement_adm_ref_count(Borrowed::from_ptr(adm).to_void()) }
}

#[unsafe(no_mangle)]
pub extern "C" fn cubeb_adm_callbacks() -> *const c_void {
    AUDIO_DEVICE_CBS_PTR.cast()
}

/// # Safety
/// `adm` is NULL or a live handle; `id` is NULL or a nul-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cubeb_adm_set_playout_device(
    adm: *const CubebAdm,
    id: *const c_char,
) -> i32 {
    status(
        unsafe { set_device(adm, Direction::Playout, id) },
        "cubeb_adm_set_playout_device",
    )
}

/// # Safety
/// `adm` is NULL or a live handle; `id` is NULL or a nul-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cubeb_adm_set_recording_device(
    adm: *const CubebAdm,
    id: *const c_char,
) -> i32 {
    status(
        unsafe { set_device(adm, Direction::Recording, id) },
        "cubeb_adm_set_recording_device",
    )
}

/// # Safety
/// `adm` is NULL or a live handle; `context` stays valid for `callback` until the handle is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cubeb_adm_set_devices_changed_callback(
    adm: *const CubebAdm,
    callback: Option<CubebAdmDevicesChanged>,
    context: *mut c_void,
) -> i32 {
    let observer = Box::new(DevicesChangedObserver { callback, context });
    status(
        unsafe { lock(adm) }.and_then(|mut adm| adm.register_audio_device_callback(observer)),
        "cubeb_adm_set_devices_changed_callback",
    )
}

/// # Safety
/// `adm` as for [`lock`].
unsafe fn device_count(adm: *const CubebAdm, direction: Direction) -> i16 {
    let count = unsafe { lock(adm) }
        .and_then(|mut adm| webrtc_devices(&mut adm, direction))
        .and_then(|devices| Ok(i16::try_from(devices.len())?));
    count.unwrap_or_else(|e| {
        error!("Failed to count {direction:?} devices: {e:?}");
        -1
    })
}

/// # Safety
/// `adm` as for [`lock`]; `name` and `guid` are NULL or valid for writing 128 bytes.
unsafe fn device_name(
    adm: *const CubebAdm,
    direction: Direction,
    index: u16,
    name: *mut c_char,
    guid: *mut c_char,
) -> anyhow::Result<()> {
    let devices = webrtc_devices(&mut *unsafe { lock(adm) }?, direction)?;
    let (device_name, device_id) = devices.get(usize::from(index)).ok_or_else(|| {
        anyhow!(
            "No {direction:?} device at index {index} of {}",
            devices.len()
        )
    })?;
    unsafe {
        copy_c_string(device_name, name, ADM_MAX_DEVICE_NAME_SIZE)?;
        copy_c_string(device_id, guid, ADM_MAX_GUID_SIZE)
    }
}

/// # Safety
/// `adm` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cubeb_adm_playout_devices(adm: *const CubebAdm) -> i16 {
    unsafe { device_count(adm, Direction::Playout) }
}

/// # Safety
/// `adm` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cubeb_adm_recording_devices(adm: *const CubebAdm) -> i16 {
    unsafe { device_count(adm, Direction::Recording) }
}

/// # Safety
/// `adm` is NULL or a live handle; `name` and `guid` are NULL or valid for writing 128 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cubeb_adm_playout_device_name(
    adm: *const CubebAdm,
    index: u16,
    name: *mut c_char,
    guid: *mut c_char,
) -> i32 {
    status(
        unsafe { device_name(adm, Direction::Playout, index, name, guid) },
        "cubeb_adm_playout_device_name",
    )
}

/// # Safety
/// `adm` is NULL or a live handle; `name` and `guid` are NULL or valid for writing 128 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cubeb_adm_recording_device_name(
    adm: *const CubebAdm,
    index: u16,
    name: *mut c_char,
    guid: *mut c_char,
) -> i32 {
    status(
        unsafe { device_name(adm, Direction::Recording, index, name, guid) },
        "cubeb_adm_recording_device_name",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webrtc_device_id_hashes_only_ids_that_do_not_fit() {
        assert_eq!(webrtc_device_id("alsa_output.pci"), "alsa_output.pci");
        let long = "x".repeat(200);
        assert_eq!(webrtc_device_id(&long), "fnv1a:6ee14c8caecd5c45");
        assert_eq!(webrtc_device_id(&long[..127]), &long[..127]);
        assert_ne!(webrtc_device_id(&long[..128]), webrtc_device_id(&long));
    }

    #[test]
    fn copy_c_string_cuts_at_char_boundary() {
        let mut buffer = [0x55 as c_char; 10];
        // 8 bytes of 'A', then a 2-byte character that would be cut in half.
        let src = format!("{}é", "A".repeat(8));
        unsafe { copy_c_string(&src, buffer.as_mut_ptr(), buffer.len()) }.unwrap();
        let copied = unsafe { CStr::from_ptr(buffer.as_ptr()) };
        assert_eq!(copied.to_str().unwrap(), "A".repeat(8));
        assert_eq!(buffer[9], 0x55, "nothing written after the terminator");
    }
}
