from playwright.sync_api import Locator, Page, expect

from conftest import load_expected
from mock_backend import MockBackend


def chunk_view(chunks: Locator) -> list[dict]:
    return chunks.evaluate_all("els => els.map(e => ({id: e.dataset.chunkId, text: e.textContent}))")


def test_happy_path(seed_page: Page, mock_backend: MockBackend) -> None:
    expected = load_expected("happy-path.json")
    page = seed_page
    page.goto("/")
    app = page.locator("seed-app")
    language = page.get_by_test_id("language")
    start = page.get_by_test_id("start")
    chunks = page.get_by_test_id("chunk")

    expect(app).to_have_attribute("data-state", "idle")
    language.select_option(expected["language"])
    start.click()

    expect(app).to_have_attribute("data-state", "recording")
    expect(app).to_have_attribute("data-cue", "start")
    expect(language).to_be_disabled()
    expect(start).to_be_disabled()

    # Finalized chunks arrive progressively, before Stop.
    expect(chunks).to_have_count(len(expected["chunksWhileRecording"]))
    expect(app).to_have_attribute("data-state", "recording")
    assert chunk_view(chunks) == expected["chunksWhileRecording"]

    page.get_by_test_id("stop").click()
    expect(app).to_have_attribute("data-state", "complete")
    expect(app).to_have_attribute("data-cue", "stop")
    assert chunk_view(chunks) == expected["chunksAtComplete"]
    assert page.evaluate("window.__seedEvents") == expected["eventsUntilComplete"]

    starts = chunks.evaluate_all("els => els.map(e => Number(e.dataset.startMs))")
    assert starts == sorted(starts)

    assert mock_backend.violations == []
    [recording] = mock_backend.recordings
    assert recording.language == expected["language"]
    assert recording.finished
    frames = recording.frames
    assert [frame.seq for frame in frames] == list(range(1, len(frames) + 1))
    assert frames[0].start_ms == 0
    for previous, frame in zip(frames, frames[1:]):
        assert frame.start_ms == previous.end_ms
    assert all(frame.end_ms >= frame.start_ms for frame in frames)
    assert {frame.media_type for frame in frames} == {expected["frameMediaType"]}

    # Starting again clears the previous in-memory view.
    expect(start).to_have_text("Start new recording")
    start.click()
    expect(app).to_have_attribute("data-state", "recording")
    expect(chunks).to_have_count(0)
    expect(app).to_have_attribute("data-cue-count", "3")
    assert (
        page.evaluate("window.__seedEvents")
        == expected["eventsUntilComplete"] + expected["eventsAfterStartNew"]
    )
