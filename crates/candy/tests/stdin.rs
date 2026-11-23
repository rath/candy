//! A controlling PTY keeps capability replies separate from the image on stdin.
mod common;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../candy-macos/tests/fixtures/sample.png")
}

fn terminal(args: &[&str], input: Option<&[u8]>, reply: Option<&[u8]>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_candy"));
    command
        .args(args)
        .env_remove("TMUX")
        .env("TERM", "xterm-kitty")
        .env("CANDY_APP", "/nonexistent/stdin-test/Candy.app");
    let stdin = match input {
        Some(_) => Stdio::piped(),
        None => File::open(fixture()).unwrap().into(),
    };
    let mut replied = false;
    common::run(command, Some(stdin), input, |bytes, master| {
        if !replied && bytes.windows(6).any(|s| s == b"AAAA\x1b\\") {
            if let Some(reply) = reply {
                master.write_all(reply).unwrap();
            }
            replied = true;
        }
    })
}

fn preview(output: &Output) -> image::RgbaImage {
    // A signal leaves stderr empty; the status and output length tell a
    // killed process from one that exited after writing its image.
    assert!(
        output.status.success(),
        "{:?} after {} output bytes: {}",
        output.status,
        output.stdout.len(),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let payload: String = text
        .split("\x1b_G")
        .skip(1)
        .filter_map(|chunk| {
            let (control, rest) = chunk.split_once(';')?;
            if control.contains("a=q") {
                return None;
            }
            Some(rest.split_once("\x1b\\").unwrap().0)
        })
        .collect();
    image::load_from_memory(&STANDARD.decode(payload).unwrap())
        .unwrap()
        .into_rgba8()
}

#[test]
fn terminal_preview_keeps_transparent_edge_colors() {
    let image = image::RgbaImage::from_fn(1280, 4, |x, _| {
        if x % 2 == 0 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0; 4])
        }
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let output = preview(&terminal(
        &["--display", "kitty"],
        Some(png.get_ref()),
        None,
    ));
    assert_eq!(output.dimensions(), (640, 2));
    for pixel in output.pixels() {
        assert_eq!(&pixel.0[..3], &[255, 0, 0]);
        assert!(pixel[3] > 0 && pixel[3] < 255);
    }
}

#[test]
fn piped_and_redirected_images_display_with_auto_and_explicit_kitty() {
    let input = std::fs::read(fixture()).unwrap();
    let expected = image::DynamicImage::ImageRgba8(candy_macos::load(&fixture()).unwrap())
        .resize(640, 432, image::imageops::FilterType::Triangle)
        .into_rgba8();
    let auto = terminal(&[], Some(&input), Some(b"\x1b_Gi=73921;OK\x1b\\"));
    assert_eq!(preview(&auto), expected);
    let forced = terminal(&["-", "--display", "kitty"], Some(&input), None);
    assert_eq!(preview(&forced), expected);
    assert!(!String::from_utf8_lossy(&forced.stdout).contains("a=q"));
    assert_eq!(
        preview(&terminal(&["--display", "kitty"], None, None)),
        expected
    );
    assert_eq!(
        preview(&terminal(
            &[fixture().to_str().unwrap(), "--display", "kitty"],
            Some(b"ignored"),
            None
        )),
        expected
    );
}

#[test]
fn stdin_errors_do_not_emit_an_image() {
    for input in [&b""[..], &b"not an image"[..]] {
        let output = terminal(&["--display", "kitty"], Some(input), None);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("stdin"));
    }
    // Empty stdin must be diagnosed even for an explicit GUI request.
    let empty = Command::new(env!("CARGO_BIN_EXE_candy"))
        .args(["-", "--display", "gui"])
        .output()
        .unwrap();
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("No image data"));
}

#[test]
fn redirected_stdout_rejects_stdin_graphics() {
    for args in [vec![], vec!["-", "--display", "kitty"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_candy"))
            .args(args)
            .stdin(File::open(fixture()).unwrap())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("--display gui"));
    }
}

#[test]
fn unsupported_terminal_falls_back_to_the_app_launcher() {
    let input = std::fs::read(fixture()).unwrap();
    let output = terminal(&[], Some(&input), Some(b"\x1b_Gi=73921;ENOTSUP\x1b\\"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("CANDY_APP"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("a=T"));
    let gui = terminal(&["--display", "gui"], Some(&input), None);
    assert!(!gui.status.success());
    assert!(String::from_utf8_lossy(&gui.stderr).contains("CANDY_APP"));
    assert!(gui.stdout.is_empty());
}
