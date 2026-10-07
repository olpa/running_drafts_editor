import json
from pathlib import Path

import pytest
from playwright.sync_api import Page

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
}

# Records changes of the state and cue test hooks, so that short states such
# as Finishing cannot be missed between assertions.
RECORD_EVENTS_SCRIPT = """
window.__seedEvents = [];
new MutationObserver((records) => {
  for (const record of records) {
    const element = record.target;
    if (record.attributeName === "data-state") {
      window.__seedEvents.push(["state", element.dataset.state]);
    } else if (record.attributeName === "data-cue-count") {
      window.__seedEvents.push(["cue", element.dataset.cue]);
    }
  }
}).observe(document, {
  subtree: true,
  attributes: true,
  attributeFilter: ["data-state", "data-cue-count"],
});
"""


@pytest.fixture(scope="session")
def browser_type_launch_args(browser_type_launch_args):
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
def browser_context_args(browser_context_args):
    return {**browser_context_args, "permissions": ["microphone"]}


@pytest.fixture
def seed_page(page: Page) -> Page:
    page.add_init_script(f"window.__SEED_CONFIG__ = {json.dumps(SEED_CONFIG)};")
    page.add_init_script(RECORD_EVENTS_SCRIPT)
    return page


@pytest.fixture
def mock_backend(seed_page: Page) -> MockBackend:
    backend = MockBackend.load(FIXTURES_DIR / "mock-backend.json")
    backend.install(seed_page)
    return backend


def load_expected(name: str) -> dict:
    return json.loads((EXPECTED_DIR / name).read_text(encoding="utf-8"))
