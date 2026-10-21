#ifndef JNI_WEBRTC_API_AUDIO_DEVICES_CHANGED_LISTENER_H_
#define JNI_WEBRTC_API_AUDIO_DEVICES_CHANGED_LISTENER_H_

#include "api/CubebAudioDeviceModule.h"

#include "JavaClass.h"
#include "JavaRef.h"

#include <jni.h>
#include <memory>

namespace jni
{
	class AudioDevicesChangedListener : public CubebDevicesChangedObserver
	{
		public:
			AudioDevicesChangedListener(JNIEnv * env, const JavaGlobalRef<jobject> & listener);
			~AudioDevicesChangedListener() override = default;

			// CubebDevicesChangedObserver implementation.
			void OnDevicesChanged() override;

		private:
			class JavaAudioDevicesChangedListenerClass : public JavaClass
			{
				public:
					explicit JavaAudioDevicesChangedListenerClass(JNIEnv * env);

					jmethodID onDevicesChanged;
			};

		private:
			JavaGlobalRef<jobject> listener;

			const std::shared_ptr<JavaAudioDevicesChangedListenerClass> javaClass;
	};
}

#endif
