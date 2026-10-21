#include "api/AudioDevicesChangedListener.h"
#include "JavaClasses.h"
#include "JavaUtils.h"
#include "JNI_WebRTC.h"

namespace jni
{
	AudioDevicesChangedListener::AudioDevicesChangedListener(JNIEnv * env, const JavaGlobalRef<jobject> & listener) :
		listener(listener),
		javaClass(JavaClasses::get<JavaAudioDevicesChangedListenerClass>(env))
	{
	}

	void AudioDevicesChangedListener::OnDevicesChanged()
	{
		JNIEnv * env = AttachCurrentThread();

		if (env == nullptr) {
			return;
		}

		env->CallVoidMethod(listener, javaClass->onDevicesChanged);

		// ExceptionCheck() rethrows as a C++ exception, which must not unwind
		// into the Rust frames that called this.
		if (env->ExceptionCheck()) {
			env->ExceptionDescribe();
			env->ExceptionClear();
		}
	}

	AudioDevicesChangedListener::JavaAudioDevicesChangedListenerClass::JavaAudioDevicesChangedListenerClass(JNIEnv * env)
	{
		jclass cls = FindClass(env, PKG_AUDIO"AudioDevicesChangedListener");

		onDevicesChanged = GetMethod(env, cls, "onDevicesChanged", "()V");
	}
}
