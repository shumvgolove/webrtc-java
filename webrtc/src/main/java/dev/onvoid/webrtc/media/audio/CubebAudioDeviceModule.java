package dev.onvoid.webrtc.media.audio;

import static java.util.Objects.requireNonNull;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Audio device module based on cubeb, adapted from Signal's RingRTC. Devices are
 * selected by their stable descriptor and the system default device follows the
 * operating system's default.
 * <p>
 * While a selected device is missing, the module uses the system default device
 * instead and returns to the selected device when it is back. This fallback can
 * be disabled per direction.
 * <p>
 * When the system has a default device, it comes first in each device list,
 * with a null descriptor. Selecting it selects the system default device.
 */
public class CubebAudioDeviceModule extends AudioDeviceModuleBase {

	private static final String DEFAULT_DEVICE_ID = "";

	private final ExecutorService listenerExecutor = new ThreadPoolExecutor(1, 1, 0, TimeUnit.MILLISECONDS,
			new LinkedBlockingQueue<>(), CubebAudioDeviceModule::newListenerThread, new ThreadPoolExecutor.DiscardPolicy());

	private final AtomicBoolean devicesChangedPending = new AtomicBoolean();

	private volatile AudioDevicesChangedListener devicesChangedListener;


	/**
	 * Creates the module and starts its audio worker thread.
	 *
	 * @throws Error if no audio backend can be initialized.
	 */
	public CubebAudioDeviceModule() {
		initialize(this::scheduleDevicesChanged);
	}

	/**
	 * Removes the device change listener and releases this reference to the
	 * module. A {@link dev.onvoid.webrtc.PeerConnectionFactory} created with
	 * this module keeps it running until the factory is disposed.
	 */
	@Override
	public void dispose() {
		devicesChangedListener = null;
		listenerExecutor.shutdown();

		super.dispose();
	}

	/**
	 * Selects the playout device by its descriptor.
	 *
	 * @param device The playout device, as listed by {@link #getPlayoutDevices()}.
	 *
	 * @throws NullPointerException if the device is null.
	 * @throws Error if the device is not available.
	 */
	@Override
	public void setPlayoutDevice(AudioDevice device) {
		requireNonNull(device);

		setPlayoutDeviceId(device.getDescriptor());
	}

	/**
	 * Selects the recording device by its descriptor.
	 *
	 * @param device The recording device, as listed by {@link #getRecordingDevices()}.
	 *
	 * @throws NullPointerException if the device is null.
	 * @throws Error if the device is not available.
	 */
	@Override
	public void setRecordingDevice(AudioDevice device) {
		requireNonNull(device);

		setRecordingDeviceId(device.getDescriptor());
	}

	/**
	 * Selects the system default playout device, which follows the operating
	 * system's default.
	 */
	public void setDefaultPlayoutDevice() {
		setPlayoutDeviceId(DEFAULT_DEVICE_ID);
	}

	/**
	 * Selects the system default recording device, which follows the operating
	 * system's default.
	 */
	public void setDefaultRecordingDevice() {
		setRecordingDeviceId(DEFAULT_DEVICE_ID);
	}

	/**
	 * Enables or disables the playout fallback, which is enabled by default.
	 * While the selected playout device is missing, an enabled fallback plays
	 * through the system default device; a disabled one plays through no device
	 * until the selected device is back or another one is selected.
	 *
	 * @param enabled true to use the system default device while the selected
	 *                one is missing.
	 */
	public native void setPlayoutFallbackEnabled(boolean enabled);

	/**
	 * Enables or disables the recording fallback, which is enabled by default.
	 * While the selected recording device is missing, an enabled fallback records
	 * from the system default device; a disabled one records from no device until
	 * the selected device is back or another one is selected.
	 *
	 * @param enabled true to use the system default device while the selected
	 *                one is missing.
	 */
	public native void setRecordingFallbackEnabled(boolean enabled);

	/**
	 * Gets the playout device the module currently uses, which differs from the
	 * selected one while the fallback is in effect.
	 *
	 * @return the device as listed by {@link #getPlayoutDevices()}, or null if no
	 *         device is in use.
	 */
	public native AudioDevice getPlayoutDeviceInUse();

	/**
	 * Gets the recording device the module currently uses, which differs from the
	 * selected one while the fallback is in effect.
	 *
	 * @return the device as listed by {@link #getRecordingDevices()}, or null if
	 *         no device is in use.
	 */
	public native AudioDevice getRecordingDeviceInUse();

	/**
	 * Sets the listener notified when a device list or a device in use changes,
	 * replacing any previous one.
	 *
	 * @param listener The listener, or null to remove the current one.
	 */
	public void setDevicesChangedListener(AudioDevicesChangedListener listener) {
		devicesChangedListener = listener;
	}

	// Called on the cubeb worker thread, which must not run the listener.
	private void scheduleDevicesChanged() {
		if (devicesChangedPending.compareAndSet(false, true)) {
			listenerExecutor.execute(this::notifyDevicesChanged);
		}
	}

	private void notifyDevicesChanged() {
		devicesChangedPending.set(false);

		AudioDevicesChangedListener listener = devicesChangedListener;

		if (listener != null) {
			listener.onDevicesChanged();
		}
	}

	private static Thread newListenerThread(Runnable runnable) {
		Thread thread = new Thread(runnable, "CubebAudioDeviceModule listener");
		thread.setDaemon(true);

		return thread;
	}

	private native void initialize(AudioDevicesChangedListener nativeListener);

	private native void setPlayoutDeviceId(String id);

	private native void setRecordingDeviceId(String id);

}
