import array
import math
import subprocess
import wave
from pathlib import Path

import pytest
from playwright.sync_api import BrowserType, Page, expect

from conftest import RECORD_EVENTS_SCRIPT
from mock_backend import MockBackend


def events(page: Page) -> list[list[str]]:
    return page.evaluate("window.__seedEvents")


def test_backlog_stop_preserves_three_frames_for_manual_upload(seed_page: Page, mock_backend: MockBackend) -> None:
    mock_backend.reject_frames = True
    seed_page.add_init_script("window.__SEED_CONFIG__.frameIntervalMs = 400;")
    seed_page.goto("/")
    seed_page.get_by_test_id("start").click()
    app = seed_page.locator("recording-panel")
    expect(seed_page.get_by_test_id("feedback")).to_have_text("Audio upload delayed. Recording continues.")
    expect(app).to_have_attribute("data-state", "error")
    expect(seed_page.get_by_test_id("feedback")).to_have_text("Recording stopped because audio could not be uploaded.")
    expect(seed_page.get_by_test_id("retry")).to_be_enabled()
    assert events(seed_page).count(["cue", "delayed"]) == 1
    assert events(seed_page).count(["cue", "interrupted"]) == 1
    assert events(seed_page).count(["cue", "health"]) == 0
    assert seed_page.evaluate("window.__recorders[0].state") == "inactive"
    assert seed_page.evaluate("window.__microphoneStreams[0].getAudioTracks().every(t => t.readyState === 'ended')")
    attempts = len(mock_backend.attempts)
    seed_page.wait_for_timeout(400)
    assert len(mock_backend.attempts) == attempts
    assert len(mock_backend.recordings) == 1
    mock_backend.reject_frames = False
    seed_page.get_by_test_id("retry").click()
    expect(app).to_have_attribute("data-state", "complete")
    [recording] = mock_backend.recordings
    assert [frame.seq for frame in recording.frames] == [1, 2, 3]
    assert all(frame.size > 0 for frame in recording.frames)
    assert events(seed_page).count(["cue", "health"]) == 3
    assert seed_page.evaluate("window.__recorders.length") == 1
    assert mock_backend.violations == []


def test_overlapping_late_acknowledgements_do_not_restart_capture(seed_page: Page, mock_backend: MockBackend) -> None:
    mock_backend.hold_acknowledgements = True
    seed_page.goto("/")
    seed_page.get_by_test_id("start").click()
    app = seed_page.locator("recording-panel")
    expect(app).to_have_attribute("data-state", "error")
    assert mock_backend.attempts.count(1) >= 2
    mock_backend.release_acknowledgements()
    expect(seed_page.get_by_test_id("pending")).to_have_text("1 second of audio pending")
    assert events(seed_page).count(["cue", "health"]) == 1
    assert seed_page.evaluate("window.__recorders[0].state") == "inactive"
    assert [frame.seq for frame in mock_backend.recordings[0].frames] == [1]
    seed_page.get_by_test_id("retry").click()
    expect(app).to_have_attribute("data-state", "complete")
    assert events(seed_page).count(["cue", "health"]) == 3
    assert mock_backend.violations == []
    mock_backend.release_acknowledgements()


def test_automatic_duration_stop_flushes_partial_audio(seed_page: Page, mock_backend: MockBackend) -> None:
    seed_page.add_init_script("window.__SEED_CONFIG__.maxRecordingMs = 550;")
    seed_page.goto("/")
    seed_page.get_by_test_id("start").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "complete")
    [recording] = mock_backend.recordings
    assert recording.finished
    assert len(recording.frames) == 3
    assert 0 < recording.frames[-1].end_ms - recording.frames[-1].start_ms < 200
    assert seed_page.evaluate("window.__recorders[0].state") == "inactive"
    assert events(seed_page).count(["cue", "stop"]) == 1


def test_pending_audio_warns_before_leaving(seed_page: Page, mock_backend: MockBackend) -> None:
    mock_backend.reject_frames = True
    seed_page.goto("/")
    seed_page.get_by_test_id("start").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "error")
    seed_page.once("dialog", lambda dialog: dialog.dismiss())
    with seed_page.expect_event("dialog") as info:
        seed_page.evaluate("location.href = '/?leave-test'")
    dialog = info.value
    assert dialog.type == "beforeunload"
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "error")


def test_microphone_denial_allows_a_new_attempt(seed_page: Page, mock_backend: MockBackend) -> None:
    seed_page.add_init_script("navigator.mediaDevices.getUserMedia = async () => { throw new Error('permission denied'); };")
    seed_page.goto("/")
    seed_page.get_by_test_id("start").click()
    expect(seed_page.get_by_test_id("feedback")).to_contain_text("permission denied")
    expect(seed_page.get_by_test_id("start")).to_be_enabled()
    expect(seed_page.get_by_test_id("language")).to_be_enabled()
    assert mock_backend.recordings == []


def test_continuous_audio_across_transport_boundaries(browser_type: BrowserType, browser_name: str, tmp_path: Path, base_url: str) -> None:
    if browser_name != "chromium":
        pytest.skip("The known-waveform input uses Chromium's file microphone flag.")
    # A rising-frequency tone identifies source time without relying on frame headers.
    sample_rate = 16_000
    source = tmp_path / "rising-tone.wav"
    samples = array.array("h", (
        int(8_000 * math.sin(2 * math.pi * (300 * i / sample_rate + 100 * (i / sample_rate) ** 2)))
        for i in range(sample_rate * 8)
    ))
    with wave.open(str(source), "wb") as output:
        output.setparams((1, 2, sample_rate, len(samples), "NONE", "not compressed"))
        output.writeframes(samples.tobytes())
    browser = browser_type.launch(args=[
        "--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream",
        f"--use-file-for-fake-audio-capture={source}",
    ])
    try:
        page = browser.new_page(base_url=base_url, permissions=["microphone"])
        page.add_init_script(RECORD_EVENTS_SCRIPT)
        page.add_init_script("""
          window.__SEED_CONFIG__ = {frameIntervalMs: 250, maxRecordingMs: 2200, pollIntervalMs: 50};
          const capture = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
          navigator.mediaDevices.getUserMedia = options => capture({audio: {
            ...options.audio, autoGainControl: false, noiseSuppression: false,
          }});
        """)
        backend = MockBackend.load(Path(__file__).parent / "fixtures/mock-backend.json")
        backend.install(page)
        page.goto("/")
        page.get_by_test_id("start").click()
        expect(page.locator("recording-panel")).to_have_attribute("data-state", "complete")
        [recording] = backend.recordings
        assert len(recording.frames) >= 8
        combined = b"".join(frame.data for frame in recording.frames)
        decoded = subprocess.run([
            "ffmpeg", "-v", "error", "-i", "pipe:0", "-f", "f32le", "-ac", "1", "-ar", "16000", "pipe:1",
        ], input=combined, capture_output=True, check=True)
        pcm = array.array("f", decoded.stdout)
        assert abs(len(pcm) / sample_rate - 2.2) < 0.15
        # Locate the tone in successive windows. Missing/duplicated sound would make
        # source time jump relative to decoded time, including at upload boundaries.
        offsets = []
        for center in range(sample_rate // 5, len(pcm) - sample_rate // 10, sample_rate // 10):
            left, right = center - 640, center + 640
            crossings = [
                i + (-pcm[i]) / (pcm[i + 1] - pcm[i])
                for i in range(left, right - 1) if pcm[i] <= 0 < pcm[i + 1]
            ]
            assert len(crossings) > 10
            frequency = (len(crossings) - 1) * sample_rate / (crossings[-1] - crossings[0])
            source_time = (frequency - 300) / 200
            offsets.append(source_time - center / sample_rate)
        assert max(offsets) - min(offsets) < 0.015, offsets
        assert page.evaluate("window.__recorders.length") == 1
        assert backend.violations == []
    finally:
        browser.close()


def test_microphone_interruption_preserves_final_audio(seed_page: Page, mock_backend: MockBackend) -> None:
    seed_page.add_init_script("window.__SEED_CONFIG__.maxRecordingMs = 15000;")
    seed_page.goto("/")
    seed_page.get_by_test_id("start").click()
    expect(seed_page.get_by_test_id("chunk")).to_have_count(2)
    mock_backend.hold_acknowledgements = True
    # Stop the real input track and let MediaRecorder report its native end.
    seed_page.evaluate("window.__microphoneStreams[0].getAudioTracks()[0].stop()")
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "error")
    expect(seed_page.get_by_test_id("feedback")).to_have_text("Recording stopped because the microphone became unavailable.")
    assert events(seed_page).count(["cue", "interrupted"]) == 1
    mock_backend.hold_acknowledgements = False
    seed_page.get_by_test_id("retry").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "complete")
    assert seed_page.evaluate("window.__recorders[0].state") == "inactive"
    assert mock_backend.recordings[0].finished
    assert len(mock_backend.recordings[0].frames) >= 3
    assert mock_backend.violations == []
    mock_backend.release_acknowledgements()


def test_upload_timeout_retains_audio_for_recovery(seed_page: Page, mock_backend: MockBackend) -> None:
    mock_backend.hold_acknowledgements = True
    seed_page.add_init_script("window.__SEED_CONFIG__.requestTimeoutMs = 500;")
    seed_page.goto("/")
    seed_page.get_by_test_id("start").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "error")
    expect(seed_page.get_by_test_id("log")).to_contain_text("f1 timeout")
    assert events(seed_page).count(["cue", "health"]) == 0
    mock_backend.hold_acknowledgements = False
    seed_page.get_by_test_id("retry").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "complete")
    assert [frame.seq for frame in mock_backend.recordings[0].frames] == [1, 2, 3]
    assert events(seed_page).count(["cue", "health"]) == 3
    assert mock_backend.violations == []
    mock_backend.release_acknowledgements()


def test_recording_panel_runs_without_seed_app(seed_page: Page, mock_backend: MockBackend) -> None:
    seed_page.goto("/")
    seed_page.evaluate("""() => {
      const panel = document.createElement('recording-panel');
      window.__panelChunks = [];
      panel.addEventListener('recording-event', event => {
        if (event.detail.type === 'chunks') window.__panelChunks = event.detail.chunks;
      });
      document.querySelector('seed-app').replaceWith(panel);
    }""")
    seed_page.get_by_test_id("start").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "recording")
    seed_page.wait_for_function("window.__panelChunks.length === 2")
    seed_page.get_by_test_id("stop").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "complete")
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-cue", "stop")
    assert len(mock_backend.recordings) == 1
    assert mock_backend.recordings[0].finished
    assert seed_page.evaluate("window.__panelChunks.length") == 3
    assert mock_backend.violations == []


def test_recording_panel_disconnect_releases_microphone_and_reconnects(seed_page: Page, mock_backend: MockBackend) -> None:
    seed_page.add_init_script("""
      window.__audioContexts = [];
      const OriginalAudioContext = window.AudioContext;
      window.AudioContext = class extends OriginalAudioContext {
        constructor(...args) { super(...args); window.__audioContexts.push(this); }
      };
    """)
    seed_page.goto("/")
    seed_page.get_by_test_id("start").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "recording")
    seed_page.evaluate("""() => {
      window.__detachedPanel = document.querySelector('recording-panel');
      window.__detachedPanel.remove();
    }""")
    seed_page.wait_for_function("window.__microphoneStreams[0].getAudioTracks().every(t => t.readyState === 'ended')")
    seed_page.wait_for_function("window.__audioContexts[0].state === 'closed'")
    seed_page.evaluate("document.querySelector('seed-app').prepend(window.__detachedPanel)")
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "idle")
    seed_page.get_by_test_id("start").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "recording")
    assert seed_page.evaluate("window.__microphoneStreams.length") == 2
    assert seed_page.evaluate("window.__recorders.length") == 2
    assert seed_page.evaluate("window.__audioContexts.length") == 2
    seed_page.get_by_test_id("stop").click()
    expect(seed_page.locator("recording-panel")).to_have_attribute("data-state", "complete")
    assert mock_backend.recordings[-1].finished
    assert mock_backend.violations == []
