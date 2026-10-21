#include "api/CubebAudioDeviceModule.h"

#include "api/make_ref_counted.h"

#include <utility>

namespace jni
{
	CubebAdmOwner::CubebAdmOwner(const CubebAdm * adm, std::unique_ptr<CubebDevicesChangedObserver> observer) :
		adm_(adm),
		observer_(std::move(observer))
	{
	}

	CubebAdmOwner::~CubebAdmOwner()
	{
		// This is the only reference, so releasing it joins the cubeb worker
		// thread before the observer it calls is destroyed.
		cubeb_adm_release(adm_);
	}

	void CubebAdmOwner::OnDevicesChanged(void * context)
	{
		static_cast<CubebAdmOwner *>(context)->observer_->OnDevicesChanged();
	}

	webrtc::scoped_refptr<CubebAudioDeviceModule> CubebAudioDeviceModule::Create(std::unique_ptr<CubebDevicesChangedObserver> observer)
	{
		const CubebAdm * adm = cubeb_adm_create();

		if (adm == nullptr) {
			return nullptr;
		}

		auto module = webrtc::make_ref_counted<CubebAudioDeviceModule>(adm, std::move(observer));

		if (module->InitInternal() != 0) {
			return nullptr;
		}

		CubebAdmOwner * owner = module.get();

		if (cubeb_adm_set_devices_changed_callback(adm, &CubebAdmOwner::OnDevicesChanged, owner) != 0) {
			return nullptr;
		}

		return module;
	}

	CubebAudioDeviceModule::CubebAudioDeviceModule(const CubebAdm * adm, std::unique_ptr<CubebDevicesChangedObserver> observer) :
		CubebAdmOwner(adm, std::move(observer)),
		RingRTCAudioDeviceModule(const_cast<CubebAdm *>(adm), static_cast<const AudioDeviceCallbacks *>(cubeb_adm_callbacks()))
	{
	}

	int16_t CubebAudioDeviceModule::PlayoutDevices()
	{
		return cubeb_adm_playout_devices(adm_);
	}

	int16_t CubebAudioDeviceModule::RecordingDevices()
	{
		return cubeb_adm_recording_devices(adm_);
	}

	int32_t CubebAudioDeviceModule::PlayoutDeviceName(uint16_t index,
													  char name[webrtc::kAdmMaxDeviceNameSize],
													  char guid[webrtc::kAdmMaxGuidSize])
	{
		return cubeb_adm_playout_device_name(adm_, index, name, guid);
	}

	int32_t CubebAudioDeviceModule::RecordingDeviceName(uint16_t index,
														char name[webrtc::kAdmMaxDeviceNameSize],
														char guid[webrtc::kAdmMaxGuidSize])
	{
		return cubeb_adm_recording_device_name(adm_, index, name, guid);
	}
}
