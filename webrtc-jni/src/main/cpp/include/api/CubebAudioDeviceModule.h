#ifndef JNI_WEBRTC_API_CUBEB_AUDIO_DEVICE_MODULE_H_
#define JNI_WEBRTC_API_CUBEB_AUDIO_DEVICE_MODULE_H_

#include "api/CubebAudioDeviceModuleApi.h"
#include "rffi/src/audio_device.h"

#include "api/scoped_refptr.h"

#include <cstdint>
#include <memory>

namespace jni
{
	class CubebDevicesChangedObserver
	{
		public:
			virtual ~CubebDevicesChangedObserver() = default;

			// Called on the cubeb worker thread.
			virtual void OnDevicesChanged() = 0;
	};

	// Holds the Rust module reference and the device change observer. Being the
	// first base of CubebAudioDeviceModule, it is destroyed after
	// RingRTCAudioDeviceModule, whose destructor still calls into the Rust module.
	class CubebAdmOwner
	{
		protected:
			CubebAdmOwner(const CubebAdm * adm, std::unique_ptr<CubebDevicesChangedObserver> observer);
			~CubebAdmOwner();

			static void OnDevicesChanged(void * context);

			const CubebAdm * const adm_;
			const std::unique_ptr<CubebDevicesChangedObserver> observer_;
	};

	class CubebAudioDeviceModule : private CubebAdmOwner, public webrtc::rffi::RingRTCAudioDeviceModule
	{
		public:
			// Returns nullptr if cubeb cannot be initialized.
			static webrtc::scoped_refptr<CubebAudioDeviceModule> Create(std::unique_ptr<CubebDevicesChangedObserver> observer);

			CubebAudioDeviceModule(const CubebAdm * adm, std::unique_ptr<CubebDevicesChangedObserver> observer);
			~CubebAudioDeviceModule() override = default;

			// The RingRTC callback table does not list devices; the C API does.
			int16_t PlayoutDevices() override;
			int16_t RecordingDevices() override;
			int32_t PlayoutDeviceName(uint16_t index,
									  char name[webrtc::kAdmMaxDeviceNameSize],
									  char guid[webrtc::kAdmMaxGuidSize]) override;
			int32_t RecordingDeviceName(uint16_t index,
										char name[webrtc::kAdmMaxDeviceNameSize],
										char guid[webrtc::kAdmMaxGuidSize]) override;

			using CubebAdmOwner::adm_;
	};
}

#endif
