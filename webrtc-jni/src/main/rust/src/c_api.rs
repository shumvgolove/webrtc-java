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

fn select_device(
    adm: &mut AudioDeviceModule,
    direction: Direction,
    id: &str,
) -> anyhow::Result<()> {
    let devices = match direction {
        Direction::Playout => adm.get_audio_playout_devices()?,
        Direction::Recording => adm.get_audio_recording_devices()?,
    };
    if id != SYSTEM_DEFAULT_DEVICE_ID && !devices.list.iter().any(|d| d.unique_id == id) {
        bail!(
            "No {direction:?} device with ID {}",
            truncate_for_logging(id)
        );
    }
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
