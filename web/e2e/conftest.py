import json
from pathlib import Path

import pytest
from playwright.sync_api import Page, expect

from mock_backend import MockBackend

E2E_DIR = Path(__file__).parent
FIXTURES_DIR = E2E_DIR / "fixtures"
EXPECTED_DIR = E2E_DIR / "expected"

# Real speech fed to Chromium's fake microphone; Chromium loops it.
FAKE_MICROPHONE_AUDIO = FIXTURES_DIR / "speech-en.riff"

# Short limits so the journey finishes in seconds.
SEED_CONFIG = {
    "frameIntervalMs": 200,
    "maxRecordingMs": 5_000,
    "pollIntervalMs": 100,
    "retryFirstMs": 80,
    "retrySecondMs": 100,
    "retryThirdMs": 170,
    "requestTimeoutMs": 1_500,
}

# Slower limits for watching the journey with --demo.
DEMO_SEED_CONFIG = {
    "frameIntervalMs": 2_000,
    "maxRecordingMs": 60_000,
    "pollIntervalMs": 500,
}
DEMO_SLOWMO_MS = 600
DEMO_LINGER_MS = 3_000
DEMO_EXPECT_TIMEOUT_MS = 20_000


def pytest_addoption(parser):
    parser.addoption(
        "--demo",
        action="store_true",
        help="show Chromium and slow the journey down so a person can follow it",
    )


def pytest_configure(config):
    if config.getoption("demo"):
        config.option.headed = True
        if not config.option.slowmo:
            config.option.slowmo = DEMO_SLOWMO_MS
        expect.set_options(timeout=DEMO_EXPECT_TIMEOUT_MS)


# Records changes of the state and cue test hooks, so that short states such
# as Finishing cannot be missed between assertions.
RECORD_EVENTS_SCRIPT = """
window.__seedEvents = [];
window.__recorders = [];
window.__microphoneStreams = [];
const OriginalRecorder = window.MediaRecorder;
window.MediaRecorder = class extends OriginalRecorder {
  constructor(...args) { super(...args); window.__recorders.push(this); }
};
const getUserMedia = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
navigator.mediaDevices.getUserMedia = async (...args) => {
  const stream = await getUserMedia(...args);
  window.__microphoneStreams.push(stream);
  return stream;
};
new MutationObserver((records) => {
  records.forEach((record, index) => {
    const next = records.slice(index + 1).find(r =>
      r.target === record.target && r.attributeName === record.attributeName);
    const value = next ? next.oldValue : record.target.getAttribute(record.attributeName);
    window.__seedEvents.push([record.attributeName === "data-state" ? "state" : "cue", value]);
  });
}).observe(document, {
  subtree: true,
  attributes: true,
  attributeOldValue: true,
  attributeFilter: ["data-state", "data-cue"],
});
"""


@pytest.fixture(scope="session")
def browser_type_launch_args(browser_type_launch_args, browser_name):
    if browser_name == "firefox":
        return {**browser_type_launch_args, "firefox_user_prefs": {
            "media.navigator.streams.fake": True,
            "media.navigator.permission.disabled": True,
        }}
    # Chromium silently records silence when the file is missing.
    assert FAKE_MICROPHONE_AUDIO.is_file(), f"missing fake microphone audio: {FAKE_MICROPHONE_AUDIO}"
    return {
        **browser_type_launch_args,
        "args": [
            "--use-fake-device-for-media-stream",
            "--use-fake-ui-for-media-stream",
            f"--use-file-for-fake-audio-capture={FAKE_MICROPHONE_AUDIO}",
        ],
    }


@pytest.fixture(scope="session")
def browser_context_args(browser_context_args, browser_name):
    return browser_context_args if browser_name == "firefox" else {**browser_context_args, "permissions": ["microphone"]}


@pytest.fixture
def demo(pytestconfig) -> bool:
    return pytestconfig.getoption("demo")


@pytest.fixture
def linger(seed_page: Page, demo: bool):
    """Returns a function that pauses in --demo mode so the viewer can look."""

    def pause() -> None:
        if demo:
            seed_page.wait_for_timeout(DEMO_LINGER_MS)

    return pause


@pytest.fixture
def seed_page(page: Page, demo: bool) -> Page:
    config = DEMO_SEED_CONFIG if demo else SEED_CONFIG
    page.add_init_script(f"window.__SEED_CONFIG__ = {json.dumps(config)};")
    page.add_init_script(RECORD_EVENTS_SCRIPT)
    return page


@pytest.fixture
def mock_backend(seed_page: Page) -> MockBackend:
    backend = MockBackend.load(FIXTURES_DIR / "mock-backend.json")
    backend.install(seed_page)
    return backend


def load_expected(name: str) -> dict:
    return json.loads((EXPECTED_DIR / name).read_text(encoding="utf-8"))
