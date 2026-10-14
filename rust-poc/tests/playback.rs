use std::path::Path;

use running_drafts_editor::{
    chunking::SampleRange,
    session::{AudioPlayer, Ffplay, PlaybackError, PlaybackSpeed},
};

#[cfg(unix)]
fn write_executable(path: &Path, contents: &str) {
    use std::{
        fs::{self, File},
        io::Write,
        os::unix::fs::PermissionsExt,
    };

    let mut file = File::create(path).unwrap();
    file.write_all(contents.as_bytes()).unwrap();
    file.sync_all().unwrap();
    drop(file);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn retry_text_file_busy<T>(
    mut operation: impl FnMut() -> Result<T, PlaybackError>,
) -> Result<T, PlaybackError> {
    use std::time::Duration;

    for _ in 0..20 {
        match operation() {
            Err(PlaybackError::Start { source, .. }) if source.raw_os_error() == Some(26) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            result => return result,
        }
    }
    operation()
}

#[cfg(unix)]
#[test]
fn ffplay_backend_passes_precise_times_to_fake_executable() {
    use std::fs;

    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("fake-ffplay");
    let arguments = directory.path().join("arguments");
    write_executable(
        &executable,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            arguments.display()
        ),
    );

    let mut player = Ffplay::new(&executable);
    retry_text_file_busy(|| {
        player.play(
            Path::new("recording.wav"),
            16_000,
            SampleRange {
                start_sample: 1,
                end_sample: 480_002,
            },
        )
    })
    .unwrap();

    let arguments = fs::read_to_string(arguments).unwrap();
    assert!(arguments.contains("-ss\n0.000062500\n"));
    assert!(arguments.contains("-t\n30.000062500\n"));
    assert!(arguments.ends_with("-i\nrecording.wav\n"));
}

#[cfg(unix)]
#[test]
fn ffplay_backend_starts_slow_playback_and_can_stop_it() {
    use std::{fs, time::Duration};

    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("fake-ffplay");
    let arguments = directory.path().join("arguments");
    write_executable(
        &executable,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexec sleep 30\n",
            arguments.display()
        ),
    );
    let mut player = Ffplay::new(&executable);
    retry_text_file_busy(|| {
        player.start(
            Path::new("recording.wav"),
            16_000,
            SampleRange {
                start_sample: 160,
                end_sample: 480,
            },
            PlaybackSpeed::Slow,
        )
    })
    .unwrap();
    for _ in 0..100 {
        if arguments.is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(player.stop().unwrap());
    let arguments = fs::read_to_string(arguments).unwrap();
    assert!(arguments.contains("-af\natempo=0.75\n"));
    assert!(!player.stop().unwrap());
}
