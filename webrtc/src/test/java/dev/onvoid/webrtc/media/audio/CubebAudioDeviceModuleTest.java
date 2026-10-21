package dev.onvoid.webrtc.media.audio;

import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Semaphore;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BooleanSupplier;
import java.util.stream.Collectors;

import dev.onvoid.webrtc.PeerConnectionFactory;
import dev.onvoid.webrtc.RTCConfiguration;
import dev.onvoid.webrtc.RTCPeerConnection;

import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.TestInstance;
import org.junit.jupiter.api.parallel.Execution;
import org.junit.jupiter.api.parallel.ExecutionMode;

@TestInstance(TestInstance.Lifecycle.PER_CLASS)
@Execution(ExecutionMode.SAME_THREAD)
class CubebAudioDeviceModuleTest {

	private static final long EVENT_TIMEOUT_SECONDS = 10;

	private CubebAudioDeviceModule module;


	@BeforeEach
	void createModule() {
		module = new CubebAudioDeviceModule();
	}

	@AfterEach
	void disposeModule() {
		if (module != null) {
			module.dispose();
			module = null;
		}
	}

	@Test
	void playoutDevicesListDefaultFirstAndUniqueDescriptors() {
		assertDeviceList("playout", module.getPlayoutDevices());
	}

	@Test
	void recordingDevicesListDefaultFirstAndUniqueDescriptors() {
		assertDeviceList("recording", module.getRecordingDevices());
	}

	@Test
	void initialDevicesInUseAreTheListedDefaults() {
		assertDefaultInUse("playout", module.getPlayoutDevices(), module.getPlayoutDeviceInUse());
		assertDefaultInUse("recording", module.getRecordingDevices(), module.getRecordingDeviceInUse());
	}

	@Test
	void selectedPlayoutDeviceBecomesTheDeviceInUse() throws Exception {
		Semaphore changes = new Semaphore(0);
		module.setDevicesChangedListener(changes::release);

		for (AudioDevice device : module.getPlayoutDevices()) {
			module.setPlayoutDevice(device);
			awaitChange(changes, () -> device.equals(module.getPlayoutDeviceInUse()), "Playout device not in use: " + device);
		}
	}

	@Test
	void selectedRecordingDeviceBecomesTheDeviceInUse() throws Exception {
		Semaphore changes = new Semaphore(0);
		module.setDevicesChangedListener(changes::release);

		for (AudioDevice device : module.getRecordingDevices()) {
			module.setRecordingDevice(device);
			awaitChange(changes, () -> device.equals(module.getRecordingDeviceInUse()), "Recording device not in use: " + device);
		}
	}

	@Test
	void removedDeviceFallsBackToDefaultAndReturns() throws Exception {
		assumeTrue(pactlAvailable(), "Needs a PulseAudio server and pactl");

		Semaphore changes = new Semaphore(0);
		module.setDevicesChangedListener(changes::release);

		String sinkName = "webrtc_java_test_fallback";
		AudioDevice sink = selectNewSink(changes, sinkName);

		AudioDevice systemDefault = module.getPlayoutDevices().get(0);
		assertNull(systemDefault.getDescriptor(), "No system default device listed");

		awaitChange(changes, () -> systemDefault.equals(module.getPlayoutDeviceInUse()), "No fallback to the system default");

		String sinkModule = loadNullSink(sinkName);

		try {
			awaitChange(changes, () -> sink.equals(module.getPlayoutDeviceInUse()), "No return to the selected device");
		}
		finally {
			pactl("unload-module", sinkModule);
		}
	}

	@Test
	void removedDeviceWithoutFallbackLeavesNoDeviceInUse() throws Exception {
		assumeTrue(pactlAvailable(), "Needs a PulseAudio server and pactl");

		Semaphore changes = new Semaphore(0);
		module.setDevicesChangedListener(changes::release);
		module.setPlayoutFallbackEnabled(false);

		selectNewSink(changes, "webrtc_java_test_no_fallback");

		awaitChange(changes, () -> module.getPlayoutDeviceInUse() == null, "A device is still in use");
	}

	@Test
	void absentSelectedDeviceFollowsTheFallbackSetting() throws Exception {
		Semaphore changes = new Semaphore(0);
		module.setDevicesChangedListener(changes::release);

		module.setPlayoutDevice(new AudioDevice("Absent", "webrtc_java_test_absent"));
		assertDefaultInUse("playout", module.getPlayoutDevices(), module.getPlayoutDeviceInUse());

		module.setPlayoutFallbackEnabled(false);
		awaitChange(changes, () -> module.getPlayoutDeviceInUse() == null, "A device is in use without fallback");
	}

	@Test
	void selectNullDeviceThrows() {
		assertThrows(NullPointerException.class, () -> module.setPlayoutDevice(null));
		assertThrows(NullPointerException.class, () -> module.setRecordingDevice(null));
	}

	@Test
	void listenerIsCalledWhenADeviceIsAddedAndRemoved() throws Exception {
		assumeTrue(pactlAvailable(), "Needs a PulseAudio server and pactl");

		Semaphore changes = new Semaphore(0);
		module.setDevicesChangedListener(changes::release);

		String sinkName = "webrtc_java_test_added";

		String sinkModule = loadNullSink(sinkName);

		try {
			awaitChange(changes, () -> findListed(module.getPlayoutDevices(), sinkName) != null, "The added sink was not reported");
		}
		finally {
			pactl("unload-module", sinkModule);
		}

		awaitChange(changes, () -> findListed(module.getPlayoutDevices(), sinkName) == null, "The removed sink was not reported");
	}

	@Test
	void listenerRunsOnTheListenerThreadAndMayDisposeTheModule() throws Exception {
		assumeTrue(pactlAvailable(), "Needs a PulseAudio server and pactl");

		AtomicReference<String> threadName = new AtomicReference<>();
		CountDownLatch disposed = new CountDownLatch(1);

		module.setDevicesChangedListener(() -> {
			threadName.set(Thread.currentThread().getName());
			module.dispose();
			disposed.countDown();
		});

		String sinkModule = loadNullSink("webrtc_java_test_dispose");

		try {
			assertTrue(disposed.await(EVENT_TIMEOUT_SECONDS, TimeUnit.SECONDS), "The listener did not dispose the module");
		}
		finally {
			pactl("unload-module", sinkModule);
		}

		assertEquals("CubebAudioDeviceModule listener", threadName.get());
		module = null;
	}

	@Test
	void replacedListenerIsNotCalled() throws Exception {
		assumeTrue(pactlAvailable(), "Needs a PulseAudio server and pactl");

		Semaphore replacedCalls = new Semaphore(0);
		Semaphore changes = new Semaphore(0);

		module.setDevicesChangedListener(replacedCalls::release);
		module.setDevicesChangedListener(changes::release);

		String sinkName = "webrtc_java_test_replaced";

		String sinkModule = loadNullSink(sinkName);

		try {
			awaitChange(changes, () -> findListed(module.getPlayoutDevices(), sinkName) != null, "The added sink was not reported");
		}
		finally {
			pactl("unload-module", sinkModule);
		}

		assertEquals(0, replacedCalls.availablePermits());
	}

	@Test
	void useAfterDisposeThrows() {
		module.dispose();

		assertThrows(NullPointerException.class, () -> module.setDefaultPlayoutDevice());
		assertThrows(NullPointerException.class, () -> module.getPlayoutDevices());
		assertThrows(NullPointerException.class, () -> module.getPlayoutDeviceInUse());
		assertThrows(NullPointerException.class, () -> module.setPlayoutFallbackEnabled(false));
		module = null;
	}

	@Test
	void peerConnectionFactoryDisposedBeforeModule() {
		PeerConnectionFactory factory = new PeerConnectionFactory(module);

		createAndClosePeerConnection(factory);

		factory.dispose();
	}

	@Test
	void moduleDisposedBeforePeerConnectionFactory() {
		PeerConnectionFactory factory = new PeerConnectionFactory(module);

		module.setDevicesChangedListener(() -> { });
		module.dispose();
		module = null;

		// The factory still holds the module, which must stay usable until the
		// factory releases it.
		createAndClosePeerConnection(factory);

		factory.dispose();
	}

	private static void createAndClosePeerConnection(PeerConnectionFactory factory) {
		RTCPeerConnection peerConnection = factory.createPeerConnection(new RTCConfiguration(), candidate -> { });

		assertNotNull(peerConnection);

		peerConnection.close();
	}

	// Loads a null sink, selects it for playout and unloads it once it is in use.
	private AudioDevice selectNewSink(Semaphore changes, String sinkName) throws Exception {
		String sinkModule = loadNullSink(sinkName);

		try {
			awaitChange(changes, () -> findListed(module.getPlayoutDevices(), sinkName) != null, "The added sink was not reported");

			AudioDevice sink = findListed(module.getPlayoutDevices(), sinkName);

			module.setPlayoutDevice(sink);
			awaitChange(changes, () -> sink.equals(module.getPlayoutDeviceInUse()), "The selected sink is not in use");

			return sink;
		}
		finally {
			pactl("unload-module", sinkModule);
		}
	}

	private static void assertDefaultInUse(String direction, List<AudioDevice> devices, AudioDevice inUse) {
		if (!devices.isEmpty() && devices.get(0).getDescriptor() == null) {
			assertEquals(devices.get(0), inUse, "The " + direction + " system default device must be in use: " + devices);
		}
		else if (inUse != null) {
			assertTrue(devices.contains(inUse), "The " + direction + " device in use " + inUse + " is not listed: " + devices);
		}
	}

	private static void assertDeviceList(String direction, List<AudioDevice> devices) {
		assertNotNull(devices, direction + " device list");

		Set<String> descriptors = new HashSet<>();

		for (int i = 0; i < devices.size(); i++) {
			AudioDevice device = devices.get(i);

			assertNotNull(device.getName(), direction + " device name at " + i);
			assertTrue(descriptors.add(device.getDescriptor()),
					direction + " device descriptor at " + i + " is not unique: " + devices);

			if (device.getDescriptor() == null) {
				assertEquals(0, i, "The " + direction + " system default device must come first: " + devices);
			}
		}
	}

	// Waits for change notifications until the condition holds.
	static void awaitChange(Semaphore changes, BooleanSupplier condition, String message) throws InterruptedException {
		while (!condition.getAsBoolean()) {
			assertTrue(changes.tryAcquire(EVENT_TIMEOUT_SECONDS, TimeUnit.SECONDS), message);
		}
	}

	static AudioDevice findListed(List<AudioDevice> devices, String name) {
		return devices.stream()
				.filter(device -> name.equals(device.getDescriptor()) || name.equals(device.getName()))
				.findFirst()
				.orElse(null);
	}

	static boolean pactlAvailable() {
		try {
			pactl("info");
			return true;
		}
		catch (IOException | InterruptedException e) {
			return false;
		}
	}

	static String pactl(String... args) throws IOException, InterruptedException {
		List<String> command = new ArrayList<>();
		command.add("pactl");
		command.addAll(Arrays.asList(args));

		Process process = new ProcessBuilder(command).redirectErrorStream(true).start();
		String output;

		try (BufferedReader reader = new BufferedReader(new InputStreamReader(process.getInputStream(), StandardCharsets.UTF_8))) {
			output = reader.lines().collect(Collectors.joining("\n")).trim();
		}

		if (process.waitFor() != 0) {
			throw new IOException(command + " failed: " + output);
		}

		return output;
	}

	// Returns the module index; the sink is listed with its name as ID and description.
	static String loadNullSink(String name) throws IOException, InterruptedException {
		return pactl("load-module", "module-null-sink", "sink_name=" + name, "sink_properties=device.description=" + name);
	}
}
