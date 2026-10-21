#include "JNI_CubebAudioDeviceModule.h"
#include "JavaError.h"
#include "JavaRef.h"
#include "JavaString.h"
#include "JavaUtils.h"

#include "api/AudioDevicesChangedListener.h"
#include "api/CubebAudioDeviceModule.h"

#include "media/audio/AudioDevice.h"

#include "api/scoped_refptr.h"

#include <cstring>
#include <memory>
#include <string>

namespace
{
	// The handle points at the AudioDeviceModule subobject, as the base class and
	// PeerConnectionFactory expect, which is not at the start of this class.
	jni::CubebAudioDeviceModule * GetCubebModule(JNIEnv * env, jobject caller)
	{
		webrtc::AudioDeviceModule * audioModule = GetHandle<webrtc::AudioDeviceModule>(env, caller);

		return static_cast<jni::CubebAudioDeviceModule *>(audioModule);
	}

	using DeviceInUseFunction = int32_t (*)(const CubebAdm *, char *, size_t);
	using DeviceCountFunction = int16_t (*)(const CubebAdm *);
	using DeviceNameFunction = int32_t (*)(const CubebAdm *, uint16_t, char *, char *);

	jobject GetDeviceInUse(JNIEnv * env, jobject caller, DeviceInUseFunction getInUse, DeviceCountFunction getCount,
						   DeviceNameFunction getName, jni::avdev::AudioDeviceDirectionType direction)
	{
		jni::CubebAudioDeviceModule * audioModule = GetCubebModule(env, caller);
		CHECK_HANDLEV(audioModule, nullptr);

		char id[webrtc::kAdmMaxGuidSize] = {};
		int32_t result = getInUse(audioModule->adm_, id, sizeof(id));

		if (result < 0) {
			env->Throw(jni::JavaError(env, "Get device in use failed"));
			return nullptr;
		}
		if (result == CUBEB_ADM_NO_DEVICE_IN_USE) {
			return nullptr;
		}

		char name[webrtc::kAdmMaxDeviceNameSize];
		char guid[webrtc::kAdmMaxGuidSize];
		int16_t deviceCount = getCount(audioModule->adm_);

		// The system default device is listed with the same empty ID.
		for (int16_t i = 0; i < deviceCount; ++i) {
			if (getName(audioModule->adm_, i, name, guid) == 0 && std::strcmp(guid, id) == 0) {
				auto device = std::make_shared<jni::avdev::AudioDevice>(name, guid);
				device->directionType = direction;

				return jni::AudioDevice::toJavaAudioDevice(env, device).release();
			}
		}

		return nullptr;
	}
}

JNIEXPORT void JNICALL Java_dev_onvoid_webrtc_media_audio_CubebAudioDeviceModule_initialize
(JNIEnv * env, jobject caller, jobject jListener)
{
	try {
		auto listener = std::make_unique<jni::AudioDevicesChangedListener>(env, jni::JavaGlobalRef<jobject>(env, jListener));
		webrtc::scoped_refptr<jni::CubebAudioDeviceModule> audioModule = jni::CubebAudioDeviceModule::Create(std::move(listener));

		if (!audioModule) {
			env->Throw(jni::JavaError(env, "Create CubebAudioDeviceModule failed"));
			return;
		}

		SetHandle(env, caller, static_cast<webrtc::AudioDeviceModule *>(audioModule.release()));
	}
	catch (...) {
		ThrowCxxJavaException(env);
	}
}

JNIEXPORT void JNICALL Java_dev_onvoid_webrtc_media_audio_CubebAudioDeviceModule_setPlayoutDeviceId
(JNIEnv * env, jobject caller, jstring jId)
{
	jni::CubebAudioDeviceModule * audioModule = GetCubebModule(env, caller);
	CHECK_HANDLE(audioModule);

	const std::string id = jni::JavaString::toNative(env, jni::JavaLocalRef<jstring>(env, jId));

	if (cubeb_adm_set_playout_device(audioModule->adm_, id.c_str()) != 0) {
		env->Throw(jni::JavaError(env, "Set playout device failed"));
	}
}

JNIEXPORT void JNICALL Java_dev_onvoid_webrtc_media_audio_CubebAudioDeviceModule_setRecordingDeviceId
(JNIEnv * env, jobject caller, jstring jId)
{
	jni::CubebAudioDeviceModule * audioModule = GetCubebModule(env, caller);
	CHECK_HANDLE(audioModule);

	const std::string id = jni::JavaString::toNative(env, jni::JavaLocalRef<jstring>(env, jId));

	if (cubeb_adm_set_recording_device(audioModule->adm_, id.c_str()) != 0) {
		env->Throw(jni::JavaError(env, "Set recording device failed"));
	}
}

JNIEXPORT void JNICALL Java_dev_onvoid_webrtc_media_audio_CubebAudioDeviceModule_setPlayoutFallbackEnabled
(JNIEnv * env, jobject caller, jboolean enabled)
{
	jni::CubebAudioDeviceModule * audioModule = GetCubebModule(env, caller);
	CHECK_HANDLE(audioModule);

	if (cubeb_adm_set_playout_fallback(audioModule->adm_, enabled) != 0) {
		env->Throw(jni::JavaError(env, "Set playout fallback failed"));
	}
}

JNIEXPORT void JNICALL Java_dev_onvoid_webrtc_media_audio_CubebAudioDeviceModule_setRecordingFallbackEnabled
(JNIEnv * env, jobject caller, jboolean enabled)
{
	jni::CubebAudioDeviceModule * audioModule = GetCubebModule(env, caller);
	CHECK_HANDLE(audioModule);

	if (cubeb_adm_set_recording_fallback(audioModule->adm_, enabled) != 0) {
		env->Throw(jni::JavaError(env, "Set recording fallback failed"));
	}
}

JNIEXPORT jobject JNICALL Java_dev_onvoid_webrtc_media_audio_CubebAudioDeviceModule_getPlayoutDeviceInUse
(JNIEnv * env, jobject caller)
{
	return GetDeviceInUse(env, caller, cubeb_adm_get_playout_device_in_use, cubeb_adm_playout_devices,
						  cubeb_adm_playout_device_name, jni::avdev::AudioDeviceDirectionType::adtRender);
}

JNIEXPORT jobject JNICALL Java_dev_onvoid_webrtc_media_audio_CubebAudioDeviceModule_getRecordingDeviceInUse
(JNIEnv * env, jobject caller)
{
	return GetDeviceInUse(env, caller, cubeb_adm_get_recording_device_in_use, cubeb_adm_recording_devices,
						  cubeb_adm_recording_device_name, jni::avdev::AudioDeviceDirectionType::adtCapture);
}
