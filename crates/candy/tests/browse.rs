use std::{path::Path, process::Command};

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
