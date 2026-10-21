//! Exercises the C API against a dedicated PulseAudio server that has no other clients or devices.
//! Run with PULSE_SERVER set and pactl on PATH: `cargo test --test c_api -- --ignored`.

#[path = "common/webrtc_stubs.rs"]
mod webrtc_stubs;

use std::{
    ffi::{CStr, CString, c_char, c_void},
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use cubeb_adm::{
    c_api::{
        CubebAdm, cubeb_adm_callbacks, cubeb_adm_create, cubeb_adm_get_playout_device_in_use,
        cubeb_adm_get_recording_device_in_use, cubeb_adm_playout_device_name,
        cubeb_adm_playout_devices, cubeb_adm_recording_device_name, cubeb_adm_recording_devices,
        cubeb_adm_release, cubeb_adm_set_devices_changed_callback, cubeb_adm_set_playout_device,
        cubeb_adm_set_playout_fallback, cubeb_adm_set_recording_device,
        cubeb_adm_set_recording_fallback,
    },
    webrtc::{audio_device_module_callbacks::AudioDeviceCallbacks, ptr::Borrowed},
};

// Matches kAdmMaxDeviceNameSize and kAdmMaxGuidSize in WebRTC's audio_device_defines.h.
const NAME_SIZE: usize = 128;
const EVENT_TIMEOUT: Duration = Duration::from_secs(5);
const MARKER_TIMEOUT: Duration = Duration::from_millis(100);
const MARKER_ATTEMPTS: usize = 50;
// The ADM ignores this value; the stubs stand in for the WebRTC AudioTransport.
const FAKE_AUDIO_TRANSPORT: usize = 1;
// Enough create/release cycles to hit device events during teardown without the fix.
const TEARDOWN_CYCLES: usize = 50;
const SUCCESS: i32 = 0;
const FAILURE: i32 = -1;
// CubebAdmDeviceInUse
const NO_DEVICE_IN_USE: i32 = 0;
const DEVICE_IN_USE: i32 = 1;

static PULSE_SERVER: Mutex<()> = Mutex::new(());

fn serialize() -> MutexGuard<'static, ()> {
    PULSE_SERVER.lock().unwrap_or_else(|e| e.into_inner())
}

fn pactl(args: &[&str]) -> String {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .expect("run pactl");
    assert!(
        output.status.success(),
        "pactl {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("pactl output is UTF-8")
}

/// A null sink or source module, unloaded on drop unless already unloaded.
struct Device {
    module: String,
    unloaded: bool,
}

impl Device {
    fn sink(name: &str, description: &str) -> Self {
        Self::load(&[
            "module-null-sink",
            &format!("sink_name={name}"),
            &format!("sink_properties=device.description={description}"),
        ])
    }

    fn source(name: &str, description: &str) -> Self {
        Self::load(&[
            "module-null-source",
            &format!("source_name={name}"),
            &format!("description={description}"),
        ])
    }

    fn load(module_and_arguments: &[&str]) -> Self {
        let module = pactl(&[&["load-module"], module_and_arguments].concat());
        Self {
            module: module.trim().to_string(),
            unloaded: false,
        }
    }

    fn unload(&mut self) {
        pactl(&["unload-module", &self.module]);
        self.unloaded = true;
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if !self.unloaded {
            pactl(&["unload-module", &self.module]);
        }
    }
}

/// Lines of `pactl subscribe`, to wait for server events instead of sleeping.
/// Subscribe before creating the ADM, which would see the marker devices.
struct ServerEvents {
    child: Child,
    lines: mpsc::Receiver<String>,
}

impl ServerEvents {
    fn subscribe() -> Self {
        let mut child = Command::new("pactl")
            .arg("subscribe")
            .stdout(Stdio::piped())
            .spawn()
            .expect("run pactl subscribe");
        let stdout = child.stdout.take().expect("pactl stdout");
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let events = Self { child, lines };
        // pactl subscribes some time after it starts; repeat a marker until it is seen.
        assert!(
            (0..MARKER_ATTEMPTS).any(|_| events.marker_seen()),
            "pactl subscribe saw no marker"
        );
        events
    }

    fn marker_seen(&self) -> bool {
        let _marker = Device::sink("adm_subscribe_marker", "Marker");
        loop {
            match self.lines.recv_timeout(MARKER_TIMEOUT) {
                Ok(line) if line.contains("'new' on sink") => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    }

    fn wait_for(&self, event: &str) {
        loop {
            match self.lines.recv_timeout(EVENT_TIMEOUT) {
                Ok(line) if line.contains(event) => return,
                Ok(_) => {}
                Err(e) => panic!("no {event:?} event: {e}"),
            }
        }
    }
}

impl Drop for ServerEvents {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Maps the device index column of `pactl list short <streams>` to names from `pactl list short <devices>`.
fn stream_devices(streams: &str, devices: &str) -> Vec<String> {
    let names = pactl(&["list", "short", devices]);
    pactl(&["list", "short", streams])
        .lines()
        .map(|stream| {
            let index = stream.split('\t').nth(1).expect("device column");
            names
                .lines()
                .find(|device| device.split('\t').next() == Some(index))
                .and_then(|device| device.split('\t').nth(1))
                .expect("device of stream")
                .to_string()
        })
        .collect()
}

/// The `Corked` state of each source-output, from `pactl list source-outputs`.
fn recording_corked() -> Vec<bool> {
    pactl(&["list", "source-outputs"])
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Corked: "))
        .map(|corked| corked == "yes")
        .collect()
}

fn playout_sinks() -> Vec<String> {
    stream_devices("sink-inputs", "sinks")
}

fn recording_sources() -> Vec<String> {
    stream_devices("source-outputs", "sources")
}

#[derive(Default)]
struct Notifications {
    count: Mutex<u32>,
    changed: Condvar,
}

impl Notifications {
    fn count(&self) -> u32 {
        *self.count.lock().unwrap()
    }

    fn wait_beyond(&self, seen: u32) {
        let count = self.count.lock().unwrap();
        let (_count, timeout) = self
            .changed
            .wait_timeout_while(count, EVENT_TIMEOUT, |count| *count <= seen)
            .unwrap();
        assert!(
            !timeout.timed_out(),
            "no devices-changed callback after {seen}"
        );
    }

    /// Rechecks `done` after each devices-changed callback.
    fn wait_until(&self, mut done: impl FnMut() -> bool) {
        loop {
            let seen = self.count();
            if done() {
                return;
            }
            self.wait_beyond(seen);
        }
    }
}

extern "C" fn on_devices_changed(context: *mut c_void) {
    let notifications = unsafe { &*(context as *const Notifications) };
    *notifications.count.lock().unwrap() += 1;
    notifications.changed.notify_all();
}

struct Adm {
    handle: *const CubebAdm,
    callbacks: &'static AudioDeviceCallbacks,
}

impl Adm {
    fn create() -> Self {
        let handle = cubeb_adm_create();
        assert!(!handle.is_null(), "cubeb_adm_create failed");
        let callbacks = unsafe { &*cubeb_adm_callbacks().cast::<AudioDeviceCallbacks>() };
        Self { handle, callbacks }
    }

    fn borrowed(&self) -> Borrowed<CubebAdm> {
        Borrowed::from_ptr(self.handle)
    }

    /// None when the list changed between reading the count and a name.
    fn list(
        &self,
        count: unsafe extern "C" fn(*const CubebAdm) -> i16,
        name: unsafe extern "C" fn(*const CubebAdm, u16, *mut c_char, *mut c_char) -> i32,
    ) -> Option<Vec<(String, String)>> {
        let count = unsafe { count(self.handle) };
        let count = u16::try_from(count).unwrap_or_else(|_| panic!("device count {count}"));
        let text = |bytes: &[u8]| {
            CStr::from_bytes_until_nul(bytes)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        };
        (0..count)
            .map(|index| {
                let mut name_out = [0u8; NAME_SIZE];
                let mut guid_out = [0u8; NAME_SIZE];
                let copied = unsafe {
                    name(
                        self.handle,
                        index,
                        name_out.as_mut_ptr().cast(),
                        guid_out.as_mut_ptr().cast(),
                    )
                };
                (copied == SUCCESS).then(|| (text(&name_out), text(&guid_out)))
            })
            .collect()
    }

    fn playout_devices(&self) -> Option<Vec<(String, String)>> {
        self.list(cubeb_adm_playout_devices, cubeb_adm_playout_device_name)
    }

    fn recording_devices(&self) -> Option<Vec<(String, String)>> {
        self.list(cubeb_adm_recording_devices, cubeb_adm_recording_device_name)
    }

    fn set_playout_device(&self, id: &str) -> i32 {
        let id = CString::new(id).unwrap();
        unsafe { cubeb_adm_set_playout_device(self.handle, id.as_ptr()) }
    }

    fn set_recording_device(&self, id: &str) -> i32 {
        let id = CString::new(id).unwrap();
        unsafe { cubeb_adm_set_recording_device(self.handle, id.as_ptr()) }
    }

    fn set_devices_changed(&self, notifications: Option<&Notifications>) -> i32 {
        let context = notifications.map_or(std::ptr::null_mut(), |n| {
            std::ptr::from_ref(n).cast_mut().cast()
        });
        let callback = notifications.map(|_| on_devices_changed as extern "C" fn(*mut c_void));
        unsafe { cubeb_adm_set_devices_changed_callback(self.handle, callback, context) }
    }

    /// The ID of the device in use ("" for the system default), or None when there is none.
    fn device_in_use(
        &self,
        get: unsafe extern "C" fn(*const CubebAdm, *mut c_char, usize) -> i32,
    ) -> Option<String> {
        let mut id = [0u8; NAME_SIZE];
        let result = unsafe { get(self.handle, id.as_mut_ptr().cast(), id.len()) };
        let id = CStr::from_bytes_until_nul(&id).unwrap().to_str().unwrap();
        match result {
            DEVICE_IN_USE => Some(id.to_string()),
            NO_DEVICE_IN_USE => {
                assert_eq!(id, "");
                None
            }
            _ => panic!("device in use failed: {result}"),
        }
    }

    fn playout_in_use(&self) -> Option<String> {
        self.device_in_use(cubeb_adm_get_playout_device_in_use)
    }

    fn recording_in_use(&self) -> Option<String> {
        self.device_in_use(cubeb_adm_get_recording_device_in_use)
    }

    fn set_recording_fallback(&self, enabled: bool) {
        assert_eq!(
            unsafe { cubeb_adm_set_recording_fallback(self.handle, enabled) },
            SUCCESS
        );
    }

    fn set_playout_fallback(&self, enabled: bool) {
        assert_eq!(
            unsafe { cubeb_adm_set_playout_fallback(self.handle, enabled) },
            SUCCESS
        );
    }

    fn start_playout(&self) {
        let cbs = self.callbacks;
        assert_eq!((cbs.init)(self.borrowed(), FAKE_AUDIO_TRANSPORT), SUCCESS);
        assert_eq!((cbs.init_playout)(self.borrowed()), SUCCESS);
        assert_eq!((cbs.start_playout)(self.borrowed()), SUCCESS);
    }

    fn start_recording(&self) {
        self.init_recording();
        assert_eq!((self.callbacks.start_recording)(self.borrowed()), SUCCESS);
    }

    fn init_recording(&self) {
        let cbs = self.callbacks;
        assert_eq!((cbs.init)(self.borrowed(), FAKE_AUDIO_TRANSPORT), SUCCESS);
        assert_eq!((cbs.init_recording)(self.borrowed()), SUCCESS);
    }
}

impl Drop for Adm {
    fn drop(&mut self) {
        unsafe { cubeb_adm_release(self.handle) };
    }
}

/// Stops the device loop even when a create/release cycle panics, so the scope can join it.
struct StopOnDrop<'a>(&'a AtomicBool);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn pair(name: &str, guid: &str) -> (String, String) {
    (name.to_string(), guid.to_string())
}

fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
    entries
        .iter()
        .map(|(name, guid)| pair(name, guid))
        .collect()
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn enumerates_devices_with_default_first() {
    let _server = serialize();
    let _speaker = Device::sink("adm_speaker", "Speaker");
    let _headset = Device::sink("adm_headset", "Headset");
    let _mic = Device::source("adm_mic", "Mic");
    pactl(&["set-default-sink", "adm_headset"]);
    pactl(&["set-default-source", "adm_mic"]);
    let adm = Adm::create();

    let playout = adm.playout_devices().expect("stable playout devices");
    let recording = adm.recording_devices().expect("stable recording devices");
    println!("playout devices: {playout:?}");
    println!("recording devices: {recording:?}");

    assert_eq!(
        playout,
        pairs(&[
            ("default (Headset)", ""),
            ("Speaker", "adm_speaker"),
            ("Headset", "adm_headset"),
        ])
    );
    // Monitor sources of the sinks are not listed.
    assert_eq!(
        recording,
        pairs(&[("default (Mic)", ""), ("Mic", "adm_mic")])
    );
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn validates_device_selection() {
    let _server = serialize();
    let _speaker = Device::sink("adm_speaker", "Speaker");
    let _mic = Device::source("adm_mic", "Mic");
    let adm = Adm::create();

    assert_eq!(adm.set_playout_device(""), SUCCESS);
    assert_eq!(adm.set_playout_device("adm_speaker"), SUCCESS);
    assert_eq!(adm.set_recording_device(""), SUCCESS);
    assert_eq!(adm.set_recording_device("adm_mic"), SUCCESS);

    // IDs of absent devices are kept as the selection.
    assert_eq!(adm.set_playout_device("adm_missing"), SUCCESS);
    assert_eq!(adm.set_recording_device("adm_speaker"), SUCCESS);
    assert_eq!(adm.set_recording_device("adm_speaker.monitor"), SUCCESS);
    assert_eq!(
        unsafe { cubeb_adm_set_playout_device(adm.handle, std::ptr::null()) },
        FAILURE
    );
    assert_eq!(
        unsafe { cubeb_adm_set_playout_device(std::ptr::null(), c"".as_ptr()) },
        FAILURE
    );
    assert_eq!(
        unsafe {
            cubeb_adm_set_devices_changed_callback(std::ptr::null(), None, std::ptr::null_mut())
        },
        FAILURE
    );
    unsafe { cubeb_adm_release(std::ptr::null()) };
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn notifies_device_changes_until_removed() {
    let _server = serialize();
    let _speaker = Device::sink("adm_speaker", "Speaker");
    let _mic = Device::source("adm_mic", "Mic");
    pactl(&["set-default-sink", "adm_speaker"]);
    // Declared before the ADM, which may call them until it is released.
    let changes = Notifications::default();
    let replaced = Notifications::default();
    let last = Notifications::default();
    let adm = Adm::create();
    assert_eq!(adm.set_devices_changed(Some(&changes)), SUCCESS);

    let mut headset = Device::sink("adm_headset", "Headset");
    changes.wait_until(|| {
        adm.playout_devices()
            .is_some_and(|d| d.contains(&pair("Headset", "adm_headset")))
    });

    pactl(&["set-default-sink", "adm_headset"]);
    changes.wait_until(|| {
        adm.playout_devices()
            .is_some_and(|d| d[0] == pair("default (Headset)", ""))
    });

    headset.unload();
    changes.wait_until(|| {
        adm.playout_devices()
            == Some(pairs(&[
                ("default (Speaker)", ""),
                ("Speaker", "adm_speaker"),
            ]))
    });

    let _usb_mic = Device::source("adm_usb_mic", "USB-Mic");
    changes.wait_until(|| {
        adm.recording_devices()
            .is_some_and(|d| d.contains(&pair("USB-Mic", "adm_usb_mic")))
    });

    // A null source adds no monitor, so it causes exactly one callback.
    assert_eq!(adm.set_devices_changed(Some(&replaced)), SUCCESS);
    let _first = Device::source("adm_first", "First");
    replaced.wait_beyond(0);

    assert_eq!(adm.set_devices_changed(None), SUCCESS);
    let _ignored = Device::source("adm_ignored", "Ignored");
    // The ignored change was queued after the removal, so it can only reach `last`.
    assert_eq!(adm.set_devices_changed(Some(&last)), SUCCESS);
    let _last = Device::source("adm_last", "Last");
    last.wait_beyond(0);
    assert_eq!(replaced.count(), 1, "removed callback still called");
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn survives_device_changes_during_release() {
    let _server = serialize();
    let _speaker = Device::sink("adm_speaker", "Speaker");
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        scope.spawn(|| {
            while !stop.load(Ordering::SeqCst) {
                let _sink = Device::sink("adm_flapping_sink", "Flapping-Sink");
                let _source = Device::source("adm_flapping_source", "Flapping-Source");
            }
        });
        let _stop = StopOnDrop(&stop);
        for _ in 0..TEARDOWN_CYCLES {
            drop(Adm::create());
        }
    });
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn playout_follows_default_and_falls_back_on_removal() {
    let _server = serialize();
    let _speaker = Device::sink("adm_speaker", "Speaker");
    let _headset = Device::sink("adm_headset", "Headset");
    pactl(&["set-default-sink", "adm_speaker"]);
    let events = ServerEvents::subscribe();
    let changes = Notifications::default();
    let adm = Adm::create();
    assert_eq!(adm.set_devices_changed(Some(&changes)), SUCCESS);

    adm.start_playout();
    events.wait_for("'new' on sink-input");
    assert_eq!(playout_sinks(), ["adm_speaker"]);

    pactl(&["set-default-sink", "adm_headset"]);
    assert_eq!(
        playout_sinks(),
        ["adm_headset"],
        "stream follows the default"
    );

    let mut usb = Device::sink("adm_usb", "USB");
    changes.wait_until(|| {
        adm.playout_devices()
            .is_some_and(|d| d.contains(&pair("USB", "adm_usb")))
    });
    assert_eq!(adm.set_playout_device("adm_usb"), SUCCESS);
    events.wait_for("'new' on sink-input");
    assert_eq!(playout_sinks(), ["adm_usb"]);

    pactl(&["set-default-sink", "adm_speaker"]);
    assert_eq!(
        playout_sinks(),
        ["adm_usb"],
        "selected device ignores the default"
    );

    usb.unload();
    events.wait_for("'new' on sink-input");
    assert_eq!(
        playout_sinks(),
        ["adm_speaker"],
        "falls back to the default"
    );

    pactl(&["set-default-sink", "adm_headset"]);
    assert_eq!(
        playout_sinks(),
        ["adm_headset"],
        "follows the default after fallback"
    );

    drop(adm);
    assert_eq!(playout_sinks(), Vec::<String>::new());
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn recording_follows_default_and_falls_back_on_removal() {
    let _server = serialize();
    let _mic = Device::source("adm_mic", "Mic");
    let _headset_mic = Device::source("adm_headset_mic", "Headset-Mic");
    pactl(&["set-default-source", "adm_mic"]);
    let events = ServerEvents::subscribe();
    let changes = Notifications::default();
    let adm = Adm::create();
    assert_eq!(adm.set_devices_changed(Some(&changes)), SUCCESS);

    adm.start_recording();
    events.wait_for("'new' on source-output");
    assert_eq!(recording_sources(), ["adm_mic"]);

    pactl(&["set-default-source", "adm_headset_mic"]);
    assert_eq!(
        recording_sources(),
        ["adm_headset_mic"],
        "stream follows the default"
    );

    let mut usb_mic = Device::source("adm_usb_mic", "USB-Mic");
    changes.wait_until(|| {
        adm.recording_devices()
            .is_some_and(|d| d.contains(&pair("USB-Mic", "adm_usb_mic")))
    });
    assert_eq!(adm.set_recording_device("adm_usb_mic"), SUCCESS);
    events.wait_for("'new' on source-output");
    assert_eq!(recording_sources(), ["adm_usb_mic"]);

    usb_mic.unload();
    events.wait_for("'new' on source-output");
    assert_eq!(
        recording_sources(),
        ["adm_headset_mic"],
        "falls back to the default"
    );

    drop(adm);
    assert_eq!(recording_sources(), Vec::<String>::new());
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn lists_long_device_names_and_ids() {
    let _server = serialize();
    // 128 bytes: cutting to fit WebRTC's 128-byte buffer would split the last character.
    let description = format!("{}é", "A".repeat(126));
    let _long_name = Device::sink("adm_long_name", &description);
    // PulseAudio's longest name, which still fits the buffer with its nul terminator.
    let long_id = format!("adm_{}", "x".repeat(123));
    let _long_id = Device::sink(&long_id, "Long-ID");
    pactl(&["set-default-sink", "adm_long_name"]);
    let adm = Adm::create();

    let devices = adm.playout_devices().expect("stable playout devices");
    assert_eq!(
        devices,
        pairs(&[
            (&format!("default ({}", "A".repeat(118)), ""),
            (&"A".repeat(126), "adm_long_name"),
            ("Long-ID", &long_id),
        ])
    );
    assert_eq!(adm.set_playout_device(&long_id), SUCCESS);
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn fallback_uses_default_until_selected_device_returns() {
    let _server = serialize();
    let _mic = Device::source("adm_mic", "Mic");
    let mut usb_mic = Device::source("adm_usb_mic", "USB-Mic");
    pactl(&["set-default-source", "adm_mic"]);
    let events = ServerEvents::subscribe();
    let changes = Notifications::default();
    let adm = Adm::create();
    assert_eq!(adm.set_devices_changed(Some(&changes)), SUCCESS);
    assert_eq!(adm.set_recording_device("adm_usb_mic"), SUCCESS);
    changes.wait_until(|| adm.recording_in_use().as_deref() == Some("adm_usb_mic"));

    adm.start_recording();
    events.wait_for("'new' on source-output");
    assert_eq!(recording_sources(), ["adm_usb_mic"]);

    let before_unplug = changes.count();
    usb_mic.unload();
    changes.wait_until(|| adm.recording_in_use().as_deref() == Some(""));
    assert!(changes.count() > before_unplug);
    events.wait_for("'new' on source-output");
    assert_eq!(
        recording_sources(),
        ["adm_mic"],
        "uses the system default meanwhile"
    );
    assert_eq!(recording_corked(), [false]);

    let _usb_mic = Device::source("adm_usb_mic", "USB-Mic");
    changes.wait_until(|| adm.recording_in_use().as_deref() == Some("adm_usb_mic"));
    events.wait_for("'new' on source-output");
    assert_eq!(
        recording_sources(),
        ["adm_usb_mic"],
        "returns to the selected device"
    );
    assert_eq!(recording_corked(), [false]);
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn no_fallback_closes_microphone_until_device_returns() {
    let _server = serialize();
    let _mic = Device::source("adm_mic", "Mic");
    let mut usb_mic = Device::source("adm_usb_mic", "USB-Mic");
    pactl(&["set-default-source", "adm_mic"]);
    let events = ServerEvents::subscribe();
    let changes = Notifications::default();
    let adm = Adm::create();
    assert_eq!(adm.set_devices_changed(Some(&changes)), SUCCESS);
    adm.set_recording_fallback(false);
    assert_eq!(adm.set_recording_device("adm_usb_mic"), SUCCESS);
    changes.wait_until(|| adm.recording_in_use().as_deref() == Some("adm_usb_mic"));

    adm.start_recording();
    events.wait_for("'new' on source-output");
    assert_eq!(recording_sources(), ["adm_usb_mic"]);

    usb_mic.unload();
    changes.wait_until(|| adm.recording_in_use().is_none());
    events.wait_for("'remove' on source-output");
    assert_eq!(
        recording_sources(),
        Vec::<String>::new(),
        "no microphone open"
    );

    // Enabling fallback uses the system default, and disabling it closes it again.
    adm.set_recording_fallback(true);
    changes.wait_until(|| adm.recording_in_use().as_deref() == Some(""));
    events.wait_for("'new' on source-output");
    assert_eq!(recording_sources(), ["adm_mic"]);
    adm.set_recording_fallback(false);
    changes.wait_until(|| adm.recording_in_use().is_none());
    events.wait_for("'remove' on source-output");
    assert_eq!(recording_sources(), Vec::<String>::new());

    let mut usb_mic = Device::source("adm_usb_mic", "USB-Mic");
    changes.wait_until(|| adm.recording_in_use().as_deref() == Some("adm_usb_mic"));
    events.wait_for("'new' on source-output");
    assert_eq!(recording_sources(), ["adm_usb_mic"]);
    assert_eq!(
        recording_corked(),
        [false],
        "recording was started, so it resumes"
    );

    // Initialized but never started: it returns corked.
    assert_eq!((adm.callbacks.stop_recording)(adm.borrowed()), SUCCESS);
    adm.init_recording();
    events.wait_for("'new' on source-output");
    usb_mic.unload();
    changes.wait_until(|| adm.recording_in_use().is_none());
    events.wait_for("'remove' on source-output");
    let _usb_mic = Device::source("adm_usb_mic", "USB-Mic");
    changes.wait_until(|| adm.recording_in_use().as_deref() == Some("adm_usb_mic"));
    events.wait_for("'new' on source-output");
    assert_eq!(recording_sources(), ["adm_usb_mic"]);
    assert_eq!(recording_corked(), [true], "recording was not started");
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn empty_device_list_keeps_selection() {
    let _server = serialize();
    let mut speaker = Device::sink("adm_speaker", "Speaker");
    let mut usb = Device::sink("adm_usb", "USB");
    pactl(&["set-default-sink", "adm_speaker"]);
    let events = ServerEvents::subscribe();
    let changes = Notifications::default();
    let adm = Adm::create();
    assert_eq!(adm.set_devices_changed(Some(&changes)), SUCCESS);
    adm.set_playout_fallback(true);
    assert_eq!(adm.set_playout_device("adm_usb"), SUCCESS);
    changes.wait_until(|| adm.playout_in_use().as_deref() == Some("adm_usb"));
    adm.start_playout();
    events.wait_for("'new' on sink-input");

    // An audio server restart briefly lists no devices.
    usb.unload();
    speaker.unload();
    changes.wait_until(|| adm.playout_devices() == Some(Vec::new()));
    assert_eq!(adm.playout_in_use().as_deref(), Some(""));

    let _speaker = Device::sink("adm_speaker", "Speaker");
    let _usb = Device::sink("adm_usb", "USB");
    changes.wait_until(|| adm.playout_in_use().as_deref() == Some("adm_usb"));
    events.wait_for("'new' on sink-input");
    assert_eq!(playout_sinks(), ["adm_usb"]);
}

#[test]
#[ignore = "needs a dedicated PulseAudio server"]
fn system_default_in_use_when_created() {
    // Enough creations to catch a selection still queued when create returns.
    const CREATIONS: usize = 20;
    let _server = serialize();
    let _speaker = Device::sink("adm_speaker", "Speaker");
    let _mic = Device::source("adm_mic", "Mic");
    for _ in 0..CREATIONS {
        let adm = Adm::create();
        assert_eq!(adm.playout_in_use().as_deref(), Some(""));
        assert_eq!(adm.recording_in_use().as_deref(), Some(""));
    }
    // Also with no devices listed at all.
    drop((_speaker, _mic));
    let adm = Adm::create();
    assert_eq!(adm.playout_in_use().as_deref(), Some(""));
    assert_eq!(adm.recording_in_use().as_deref(), Some(""));
}
