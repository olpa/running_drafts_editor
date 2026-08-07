# RDE vendored Whisper sources

This directory is a source-only vendor snapshot used to build RDE without
external native-library environment variables.

- `whisper-rs`: <https://github.com/olpa/whisper-rs>, commit
  `3ef7217afcf75841380524870625a835bcfdf803` (`backtrack`).
- `sys/whisper.cpp`: <https://github.com/olpa/whisper.cpp>, commit
  `5ae298ef696d454f458a10160afcd877fff19170` (`backtrack`).

The Rust snapshot's prebuilt-library build files were replaced with the
self-contained static CMake build from its parent before commit `f784b74`
(the commit that removed the original `sys/whisper.cpp` submodule). RDE also
allows the conditional bindgen builder to be mutable without warning when no
GPU feature is selected.

The upstream `models/for-tests-*.bin` miniature model fixtures are omitted;
RDE keeps recognition models external.

To refresh this snapshot, reproduce the exact source archives, preserve the
local static-build adaptation, update both commit IDs here, and run the complete
RDE test and real-audition checks. Do not copy either source repository's
`.git` directory or build outputs.
