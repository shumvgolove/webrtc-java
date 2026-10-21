#include "JNI_CubebAudioDeviceModule.h"
#include "JavaError.h"
#include "JavaRef.h"
#include "JavaString.h"
#include "JavaUtils.h"

#include "api/AudioDevicesChangedListener.h"
#include "api/CubebAudioDeviceModule.h"

#include "api/scoped_refptr.h"

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
