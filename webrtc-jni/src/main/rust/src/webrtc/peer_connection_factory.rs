//
// Copyright 2019-2021 Signal Messenger, LLC
// SPDX-License-Identifier: AGPL-3.0-only
//

//! WebRTC Peer Connection

use std::fmt::Formatter;

use crate::core::util::truncate_for_logging;

/// Describes an audio input or output device.
#[derive(Clone, PartialEq, Eq)]
pub struct AudioDevice {
    /// Name of the device
    pub name: String,
    /// Unique ID
    pub unique_id: String,
}
impl std::fmt::Debug for AudioDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AudioDevice{{ name: {}, unique_id: {} }}",
            truncate_for_logging(&self.name),
            truncate_for_logging(&self.unique_id)
        )
    }
}

// See https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-roles
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsDefaultAudioDevice {
    // E.g. for VoIP. historically labeled "communications"
    pub communications_device: Option<AudioDevice>,
    // E.g. for games, system notifications. historically labeled "default"
    pub console_device: Option<AudioDevice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultAudioDevice {
    Windows(WindowsDefaultAudioDevice),
    NonWindows(Option<AudioDevice>),
}

impl DefaultAudioDevice {
    pub fn empty_windows() -> Self {
        DefaultAudioDevice::Windows(WindowsDefaultAudioDevice {
            communications_device: None,
            console_device: None,
        })
    }

    pub fn empty_non_windows() -> Self {
        DefaultAudioDevice::NonWindows(None)
    }

    pub fn default_for_target() -> Self {
        if cfg!(target_os = "windows") {
            Self::empty_windows()
        } else {
            Self::empty_non_windows()
        }
    }

    // get the default device for VoIP, if any exists.
    pub fn default_for_voip(&self) -> Option<AudioDevice> {
        match self {
            Self::Windows(WindowsDefaultAudioDevice {
                communications_device,
                ..
            }) => communications_device.clone(),
            Self::NonWindows(device) => device.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDeviceList {
    pub list: Vec<AudioDevice>,
    pub default: DefaultAudioDevice,
}

impl AudioDeviceList {
    pub fn new_for_target_os() -> Self {
        Self::new_with_default(DefaultAudioDevice::default_for_target())
    }
    pub fn new_with_default(default: DefaultAudioDevice) -> Self {
        Self {
            list: Vec::new(),
            default,
        }
    }
}

// An observer trait that receives notifications whenever the input or output
// devices change.
// These callbacks should run "quickly", as they'll be run from the cubeb worker
// thread, and any delays will block playout_delay calls, which happen
// constantly during calls (short blocking is OK, as is passing the
// event to another thread; calling a client directly is not).
// Currently only applicable on Desktop
pub trait AudioDeviceObserver: Send + std::fmt::Debug {
    fn output_changed(&self, devices: AudioDeviceList);
    fn input_changed(&self, devices: AudioDeviceList);
    fn settle_promise(&self, success: bool, audio_device: Option<AudioDevice>, request_id: u32);
    fn settle_start_recording_via_callback(&self, success: bool, audio_device: Option<AudioDevice>);
    fn settle_start_playback_via_callback(&self, success: bool, audio_device: Option<AudioDevice>);
}
