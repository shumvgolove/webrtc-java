#[macro_use]
extern crate log;

pub mod c_api;

pub mod core {
    pub mod util;
}

// Signal's audio device module from the ringrtc submodule, at RingRTC's module paths.
pub mod webrtc {
    #[path = "../../third_party/ringrtc/src/rust/src/webrtc/audio_device_module.rs"]
    pub mod audio_device_module;
    #[path = "../../third_party/ringrtc/src/rust/src/webrtc/audio_device_module_callbacks.rs"]
    pub mod audio_device_module_callbacks;
    #[path = "../../third_party/ringrtc/src/rust/src/webrtc/audio_device_module_utils.rs"]
    pub mod audio_device_module_utils;
    pub mod peer_connection_factory;
    #[path = "../../third_party/ringrtc/src/rust/src/webrtc/ptr.rs"]
    pub mod ptr;
    // A #[path] inside a nested inline module resolves from src/webrtc/ffi/, which does not exist.
    #[path = "../../third_party/ringrtc/src/rust/src/webrtc/ffi/audio_device_module.rs"]
    mod ffi_audio_device_module;
    mod ffi {
        pub(super) use super::ffi_audio_device_module as audio_device_module;
    }
}

#[cfg(test)]
#[path = "../tests/common/webrtc_stubs.rs"]
mod webrtc_stubs;
