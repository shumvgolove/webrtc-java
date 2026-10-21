//! Stand-ins for the exports of the C++ bridge, which the ADM calls while streams run.

use std::ffi::c_void;

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn Rust_recordedDataIsAvailable(
    _audio_callback_ptr_ptr: usize,
    _audio_samples: *const c_void,
    _n_samples: usize,
    _n_bytes_per_sample: usize,
    _n_channels: usize,
    _samples_per_sec: u32,
    _total_delay_ms: u32,
    _clock_drift: i32,
    _current_mic_level: u32,
    _key_pressed: bool,
    _new_mic_level: *mut u32,
    _estimated_capture_time_ns: i64,
) -> i32 {
    0
}

/// # Safety
/// The ADM passes a buffer of `n_samples * n_bytes_per_sample * n_channels` bytes and valid out pointers.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn Rust_needMorePlayData(
    _audio_callback_ptr_ptr: usize,
    n_samples: usize,
    n_bytes_per_sample: usize,
    n_channels: usize,
    _samples_per_sec: u32,
    audio_samples: *mut c_void,
    n_samples_out: *mut usize,
    _elapsed_time_ms: *mut i64,
    _ntp_time_ms: *mut i64,
) -> i32 {
    unsafe {
        std::ptr::write_bytes(
            audio_samples.cast::<u8>(),
            0,
            n_samples * n_bytes_per_sample * n_channels,
        );
        *n_samples_out = n_samples;
    }
    0
}
