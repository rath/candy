//! A controlling PTY keeps capability replies separate from the image on stdin.
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../candy-macos/tests/fixtures/sample.png")
}

fn terminal(args: &[&str], input: Option<&[u8]>, reply: Option<&[u8]>) -> Output {
    let (mut master, mut slave) = (0, 0);
    let mut size = libc::winsize {
        ws_row: 30,
        ws_col: 80,
        ws_xpixel: 640,
        ws_ypixel: 480,
    };
    // SAFETY: openpty initializes both descriptors; each is owned by one File.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        },
        0
    );
    let (mut master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
    let mut command = Command::new(env!("CARGO_BIN_EXE_candy"));
    command
        .args(args)
        .env_remove("TMUX")
        .env("TERM", "xterm-kitty")
        .env("CANDY_APP", "/nonexistent/stdin-test/Candy.app")
        .stdin(match input {
            Some(_) => Stdio::piped(),
            None => File::open(fixture()).unwrap().into(),
        })
        .stdout(slave)
        .stderr(Stdio::piped());
    // SAFETY: only async-signal-safe libc calls run between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1
                || libc::ioctl(libc::STDOUT_FILENO, libc::TIOCSCTTY.into(), 0) == -1
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().unwrap();
    drop(command);
    if let Some(input) = input {
        // The explicit-path test may exit without consuming stdin.
        let _ = child.stdin.take().unwrap().write_all(input);
    }
    let mut bytes = Vec::new();
    let mut replied = false;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let mut poll = libc::pollfd {
            fd: master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut poll, 1, 20) } > 0 {
            let mut buffer = [0; 8192];
            match master.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => bytes.extend_from_slice(&buffer[..n]),
                Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
                Err(e) => panic!("PTY read: {e}"),
            }
            if !replied && bytes.windows(6).any(|s| s == b"AAAA\x1b\\") {
                if let Some(reply) = reply {
                    master.write_all(reply).unwrap();
                }
                replied = true;
            }
        } else if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("stdin display timed out");
        }
    }
    let mut output = child.wait_with_output().unwrap();
    output.stdout = bytes;
    output
}

fn preview(output: &Output) -> image::RgbaImage {
    assert!(
        output.status.success(),
        "{}",
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
