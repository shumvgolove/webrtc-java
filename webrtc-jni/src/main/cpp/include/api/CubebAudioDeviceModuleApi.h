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

// An empty id selects the system default device, which follows the OS default.
int32_t cubeb_adm_set_playout_device(const CubebAdm* adm, const char* id);
int32_t cubeb_adm_set_recording_device(const CubebAdm* adm, const char* id);

// Called from the cubeb worker thread after the device list changes; NULL removes the callback.
int32_t cubeb_adm_set_devices_changed_callback(const CubebAdm* adm, CubebAdmDevicesChanged callback, void* context);

#ifdef __cplusplus
}
#endif

#endif
