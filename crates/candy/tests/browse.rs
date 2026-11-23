mod common;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

fn failure(args: &[&str], path: &Path, expected: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_candy"))
        .args(args)
        .arg(path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.starts_with("candy: ") && error.contains(expected),
        "{error}"
    );
}

#[test]
fn preflight_errors_precede_terminal_output_for_both_entry_points() {
    let dir = tempfile::tempdir().unwrap();
    failure(&["browse"], &dir.path().join("missing"), "Cannot open");
    for args in [&["browse"][..], &[][..]] {
        failure(args, dir.path(), "No images in");
    }
    std::fs::write(dir.path().join("a.png"), []).unwrap();
    for args in [&["browse"][..], &[][..]] {
        failure(
            args,
            dir.path(),
            "The browser needs an interactive terminal",
        );
    }
    for args in [
        &["browse", "--display", "gui"][..],
        &["--display", "gui"][..],
    ] {
        failure(
            args,
            dir.path(),
            "The browser runs in the terminal; use `candy edit <file>` for the editor",
        );
    }
}

fn browser(display: &str) -> Command {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../candy-macos/tests/fixtures");
    let mut command = Command::new(env!("CARGO_BIN_EXE_candy"));
    command
        .args(["browse", "--display", display])
        .arg(fixtures)
        .env_remove("TMUX")
        .env("TERM", "xterm-kitty");
    command
}

#[test]
fn previews_replace_the_previous_id_and_quit_restores_the_screen() {
    let mut navigated = false;
    let mut quit = false;
    let mut command = browser("kitty");
    command.env("NO_COLOR", "1");
    let output = common::run(command, None, None, |bytes, master| {
        if !navigated && bytes.windows(4).any(|s| s == "\u{10eeee}".as_bytes()) {
            master.write_all(b"j").unwrap();
            navigated = true;
        } else if navigated && !quit && bytes.windows(7).any(|s| s == b"a=d,d=I") {
            master.write_all(b"q").unwrap();
            quit = true;
        }
    });
    assert!(
        output.status.success(),
        "{:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    // Each frame is one synchronized update, so no image is painted halfway.
    let first_cell = text.find('\u{10eeee}').unwrap();
    let begin = text[..first_cell].rfind("\x1b[?2026h").unwrap();
    assert!(!text[begin..first_cell].contains("\x1b[?2026l"));
    assert!(text[first_cell..].contains("\x1b[?2026l"));
    let ids: Vec<_> = text
        .split("a=T,U=1,i=")
        .skip(1)
        .map(|s| s.split_once(',').unwrap().0)
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    for id in &ids {
        let color = id.parse::<u32>().unwrap() as u8;
        assert!(
            text.contains(&format!("38;5;{color}")),
            "missing indexed placement color"
        );
    }
    let replacement = text.find(&format!("a=T,U=1,i={}", ids[1])).unwrap();
    let painted = replacement + text[replacement..].find('\u{10eeee}').unwrap();
    let old_delete = text.find(&format!("a=d,d=I,i={},q=2", ids[0])).unwrap();
    let last_delete = text.find(&format!("a=d,d=I,i={},q=2", ids[1])).unwrap();
    assert!(
        painted < old_delete
            && old_delete < last_delete
            && last_delete < text.find("\x1b[?1049l").unwrap()
    );
    for chunk in text.split("\x1b_G").skip(1) {
        assert!(chunk.split([';', '\x1b']).next().unwrap().contains("q=2"));
    }
}

#[test]
fn graphics_without_a_placeholder_terminal_fails_before_the_alternate_screen() {
    let mut answered = false;
    let output = common::run(browser("auto"), None, None, |bytes, master| {
        if !answered && bytes.ends_with(b"\x1b[>0q") {
            master
                .write_all(b"\x1b_Gi=73921;OK\x1b\\\x1bP>|other 1.0\x1b\\")
                .unwrap();
            answered = true;
        }
    });
    assert!(answered);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unicode placeholders"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("\x1b[?1049h"));
}

#[test]
fn a_redirected_keyboard_fails_before_capture() {
    let output = common::run(
        browser("kitty"),
        Some(std::process::Stdio::null()),
        None,
        |_, _| {},
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("interactive terminal"));
}

#[test]
fn a_dropped_tmux_file_retries_once_and_the_next_upload_can_quit_immediately() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    };
    let temp = tempfile::tempdir().unwrap();
    let tmux = temp.path().join("tmux");
    std::fs::write(&tmux, "#!/bin/sh\necho on\n").unwrap();
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = browser("kitty");
    command
        .env("TMUX", "test")
        .env("PATH", temp.path())
        .env("TMPDIR", temp.path());
    for var in ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"] {
        command.env_remove(var);
    }
    let mut moved = false;
    let mut quit = false;
    let output = common::run(command, None, None, |bytes, master| {
        let text = String::from_utf8_lossy(bytes);
        if !moved && text.contains("Terminal") && text.contains("passthrough") {
            master.write_all(b"j").unwrap();
            moved = true;
        } else if moved && !quit && text.matches(",t=t;").count() == 3 {
            master.write_all(&[3]).unwrap();
            quit = true;
        }
    });
    assert!(quit && output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    // The browser rejects --display gui, so its timeout offers a retry instead.
    assert!(text.contains("retry") && !text.contains("--display"));
    let files: Vec<_> = text
        .split(",t=t;")
        .skip(1)
        .map(|chunk| {
            let name = STANDARD
                .decode(chunk.split('\x1b').next().unwrap())
                .unwrap();
            PathBuf::from(OsString::from_vec(name))
        })
        .collect();
    assert_eq!(files.len(), 3);
    assert_ne!(files[0], files[1]);
    let last_upload = text.rsplit_once(",t=t;").unwrap().1;
    assert_eq!(last_upload.matches("a=d,d=I,i=").count(), 2);
    for path in files {
        assert!(!path.exists(), "{}", path.display());
    }
}

#[test]
fn remote_tmux_sends_quiet_inline_chunks_through_passthrough() {
    use std::os::unix::fs::PermissionsExt;
    // Over SSH the terminal cannot open a local file, so each chunk travels
    // inline through its own tmux passthrough packet.
    let temp = tempfile::tempdir().unwrap();
    let tmux = temp.path().join("tmux");
    std::fs::write(&tmux, "#!/bin/sh\necho on\n").unwrap();
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = browser("kitty");
    command
        .env("TMUX", "test")
        .env("PATH", temp.path())
        .env("SSH_CONNECTION", "192.0.2.1 50000 192.0.2.2 22");
    let mut quit = false;
    let output = common::run(command, None, None, |bytes, master| {
        if !quit && bytes.windows(4).any(|s| s == "\u{10eeee}".as_bytes()) {
            master.write_all(b"q").unwrap();
            quit = true;
        }
    });
    assert!(
        quit && output.status.success(),
        "{:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains(",t=t;"));
    let packets: Vec<_> = text
        .split("\x1bPtmux;")
        .skip(1)
        .map(|packet| {
            let end = packet.find("\x1b\x1b\\\x1b\\").unwrap() + 3;
            let inner = &packet[..end];
            assert!(!inner.replace("\x1b\x1b", "").contains('\x1b'));
            inner.strip_prefix("\x1b\x1b_G").unwrap()
        })
        .collect();
    let uploads: Vec<_> = packets
        .iter()
        .filter(|packet| !packet.starts_with("a=d"))
        .collect();
    assert!(uploads.len() > 1, "a preview spans several chunks");
    assert!(uploads[0].starts_with("a=T,U=1,") && uploads[0].contains(",t=d,m=1;"));
    for packet in &packets {
        assert!(packet.split(';').next().unwrap().contains("q=2"));
    }
    assert!(packets.iter().any(|packet| packet.starts_with("a=d,d=I,")));
}
