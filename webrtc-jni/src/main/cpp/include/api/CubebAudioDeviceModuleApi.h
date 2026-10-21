#ifndef JNI_WEBRTC_API_CUBEB_AUDIO_DEVICE_MODULE_API_H_
#define JNI_WEBRTC_API_CUBEB_AUDIO_DEVICE_MODULE_API_H_

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Implemented by the Rust staticlib in webrtc-jni/src/main/rust.
// A handle is an Arc<Mutex<AudioDeviceModule>> leaked to C; every function is thread-safe.
typedef struct CubebAdm CubebAdm;

typedef void (*CubebAdmDevicesChanged)(void* context);

// Returns a handle holding one reference, or NULL if cubeb cannot be initialized.
CubebAdm* cubeb_adm_create(void);

// Drops the reference taken by cubeb_adm_create; the module stops when the last one is gone.
void cubeb_adm_release(const CubebAdm* adm);

// The AudioDeviceCallbacks table of audio_device_intf.h, called with the handle as adm_borrowed.
const void* cubeb_adm_callbacks(void);

// WebRTC's AudioDeviceModule enumeration: the system default first, as "default (<name>)" with guid
// "", then every device. name and guid are kAdmMaxDeviceNameSize and kAdmMaxGuidSize (128) bytes;
// names are cut at a character boundary, and an ID that does not fit is replaced by a stable hash
// that cubeb_adm_set_*_device accepts.
int16_t cubeb_adm_playout_devices(const CubebAdm* adm);
int16_t cubeb_adm_recording_devices(const CubebAdm* adm);
int32_t cubeb_adm_playout_device_name(const CubebAdm* adm, uint16_t index, char name[128], char guid[128]);
int32_t cubeb_adm_recording_device_name(const CubebAdm* adm, uint16_t index, char name[128], char guid[128]);

// An empty id selects the system default device, which follows the OS default.
int32_t cubeb_adm_set_playout_device(const CubebAdm* adm, const char* id);
int32_t cubeb_adm_set_recording_device(const CubebAdm* adm, const char* id);

// Called from the cubeb worker thread after the device list changes; NULL removes the callback.
int32_t cubeb_adm_set_devices_changed_callback(const CubebAdm* adm, CubebAdmDevicesChanged callback, void* context);

#ifdef __cplusplus
}
#endif

#endif
