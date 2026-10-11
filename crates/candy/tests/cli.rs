use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
fn source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../candy-macos/tests/fixtures/sample.png")
}
fn run(args: &[&str], input: &Path, output: &Path) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_candy"));
    cmd.arg(args[0])
        .arg(input)
        .arg("-o")
        .arg(output)
        .args(&args[1..]);
    cmd.output().unwrap()
}
#[test]
fn commands_produce_expected_images_and_protect_originals() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("한글 original.png");
    std::fs::copy(source(), &src).unwrap();
    let original = std::fs::read(&src).unwrap();
    let cases = [
        (vec!["resize", "--width", "500"], (500, 330)),
        (vec!["crop", "--rect", "40,50,200,100"], (200, 100)),
        (vec!["mosaic", "--rect", "40,50,200,100"], (1000, 660)),
        (vec!["flatten", "--background", "#223344"], (1000, 660)),
    ];
    for (i, (args, dimensions)) in cases.iter().enumerate() {
        let output = temp.path().join(format!("result-{i}.png"));
        let result = run(args, &src, &output);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let image = candy_macos::load(&output).unwrap();
        assert_eq!(image.dimensions(), *dimensions);
        let first = std::fs::read(&output).unwrap();
        assert!(!run(args, &src, &output).status.success());
        assert_eq!(std::fs::read(&output).unwrap(), first);
        assert!(!run(args, &src, &src).status.success());
        assert_eq!(std::fs::read(&src).unwrap(), original);
    }
}
#[test]
fn redirected_display_has_no_graphics_escape_codes() {
    let result = Command::new(env!("CARGO_BIN_EXE_candy"))
        .arg(source())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--display gui"));
}
#[test]
fn text_mosaic_changes_pixels_without_changing_dimensions() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("redacted.png");
    let result = run(&["mosaic", "--text"], &source(), &output);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let original = candy_macos::load(&source()).unwrap();
    let edited = candy_macos::load(&output).unwrap();
    assert_eq!(original.dimensions(), edited.dimensions());
    assert_ne!(original, edited);
}
