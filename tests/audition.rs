use std::path::Path;

use running_drafts_editor::{
    audition::{AudioPlayer, Ffplay},
    chunking::SampleRange,
};

#[cfg(unix)]
#[test]
fn ffplay_backend_passes_precise_times_to_fake_executable() {
    use std::{fs, os::unix::fs::PermissionsExt};

    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("fake-ffplay");
    let arguments = directory.path().join("arguments");
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            arguments.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

    let mut player = Ffplay::new(&executable);
    player
        .play(
            Path::new("recording.wav"),
            16_000,
            SampleRange {
                start_sample: 1,
                end_sample: 480_002,
            },
        )
        .unwrap();

    let arguments = fs::read_to_string(arguments).unwrap();
    assert!(arguments.contains("-ss\n0.000062500\n"));
    assert!(arguments.contains("-t\n30.000062500\n"));
    assert!(arguments.ends_with("-i\nrecording.wav\n"));
}
