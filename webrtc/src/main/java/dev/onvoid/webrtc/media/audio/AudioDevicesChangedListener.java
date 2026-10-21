package dev.onvoid.webrtc.media.audio;

/**
 * Notified when the list of playout or recording devices of a
 * {@link CubebAudioDeviceModule} has changed, for example because a device was
 * plugged in or the system default device was switched.
 */
@FunctionalInterface
public interface AudioDevicesChangedListener {

	/**
	 * The playout or recording device list has changed. Query the new lists
	 * with {@link CubebAudioDeviceModule#getPlayoutDevices()} and
	 * {@link CubebAudioDeviceModule#getRecordingDevices()}.
	 * <p>
	 * Called on a listener thread of the module, never on its audio thread.
	 * Changes that occur while this method runs are reported by one more call.
	 * The listener may query the device lists and may dispose the module.
	 */
	void onDevicesChanged();

}
