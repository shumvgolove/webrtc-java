package dev.onvoid.webrtc.media.audio;

/**
 * Notified when the playout or recording devices of a
 * {@link CubebAudioDeviceModule} have changed: a device list changed, for
 * example because a device was plugged in or the system default device was
 * switched, or a device in use changed, because the module fell back to the
 * system default device, returned to the selected device, or a fallback setting
 * was changed.
 */
@FunctionalInterface
public interface AudioDevicesChangedListener {

	/**
	 * A device list or a device in use has changed. Query the new state with
	 * {@link CubebAudioDeviceModule#getPlayoutDevices()},
	 * {@link CubebAudioDeviceModule#getRecordingDevices()},
	 * {@link CubebAudioDeviceModule#getPlayoutDeviceInUse()} and
	 * {@link CubebAudioDeviceModule#getRecordingDeviceInUse()}.
	 * <p>
	 * Called on a listener thread of the module, never on its audio thread.
	 * Changes that occur while this method runs are reported by one more call.
	 * The listener may query the device lists and may dispose the module.
	 */
	void onDevicesChanged();

}
