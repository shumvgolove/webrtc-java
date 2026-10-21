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
	void selectUnknownDeviceThrows() {
		AudioDevice unknown = new AudioDevice("Unknown", "no-such-device-id");

		assertThrows(Error.class, () -> module.setPlayoutDevice(unknown));
		assertThrows(Error.class, () -> module.setRecordingDevice(unknown));
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
