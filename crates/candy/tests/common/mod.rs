//! Run Candy on a controlling PTY and drive it through its master descriptor.
use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

pub fn run(
    mut command: Command,
    stdin: Option<Stdio>,
    input: Option<&[u8]>,
    mut interact: impl FnMut(&[u8], &mut File),
) -> Output {
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
    for fd in [master, slave] {
        assert_eq!(
            unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) },
            0
        );
    }
    let (mut master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
    command
        .stdin(stdin.unwrap_or_else(|| slave.try_clone().unwrap().into()))
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
        // An explicit-path invocation can exit without consuming its stdin.
        let _ = child.stdin.take().unwrap().write_all(input);
    }
    let mut bytes = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let mut poll = libc::pollfd {
            fd: master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut poll, 1, 20) } > 0 {
            let mut buffer = [0; 16384];
            match master.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => bytes.extend_from_slice(&buffer[..n]),
                Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
                Err(e) => panic!("PTY read: {e}"),
            }
            interact(&bytes, &mut master);
        } else if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("PTY command timed out after {} output bytes", bytes.len());
        }
    }
    let mut output = child.wait_with_output().unwrap();
    output.stdout = bytes;
    output
}
