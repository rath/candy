use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageFormat, RgbaImage, imageops::FilterType};
use std::{
    ffi::CStr,
    fs::OpenOptions,
    hash::{BuildHasher, RandomState},
    io::{Cursor, Read, Write},
    os::fd::AsRawFd,
    process::Command,
    time::{Duration, Instant},
};

const QUERY_ID: u32 = 73921;
const PLACEHOLDER: char = '\u{10EEEE}';

struct TerminalGuard {
    fd: libc::c_int,
    termios: libc::termios,
    signals: libc::sigset_t,
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Restore terminal settings before allowing a pending termination signal.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.termios);
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.signals, std::ptr::null_mut());
        }
    }
}

pub fn supported() -> bool {
    if std::env::var_os("TMUX").is_some() {
        return tmux_supported();
    }
    if std::env::var("TERM").is_ok_and(|t| t.starts_with("tmux") || t.starts_with("screen")) {
        return false;
    }
    probe().unwrap_or(false)
}

/// tmux turns a probe reply into keystrokes and drops its ESC bytes, and a
/// Kitty reply would not prove Unicode placeholder support anyway, so ask tmux
/// about the pane and the terminal its client runs in.
fn tmux_supported() -> bool {
    match (
        tmux_format("#{pane_tty}\t#{allow-passthrough}\t#{client_termtype}"),
        stdout_tty(),
    ) {
        (Some(reply), Some(tty)) => pane_shows_images(&reply, &tty),
        _ => false,
    }
}

/// Images need output that goes straight to the pane (a TMUX variable inherited
/// elsewhere names another pane), passthrough, and a client terminal that draws
/// placeholders. tmux names clients by their XTVERSION reply, such as
/// "ghostty 1.3.1" or "kitty(0.42.1)".
fn pane_shows_images(reply: &str, tty: &str) -> bool {
    let mut fields = reply.splitn(3, '\t');
    let (Some(pane_tty), Some(passthrough), Some(terminal)) =
        (fields.next(), fields.next(), fields.next())
    else {
        return false;
    };
    let name = terminal.split([' ', '(']).next().unwrap_or_default();
    pane_tty == tty
        && matches!(passthrough, "on" | "all")
        && ["ghostty", "kitty"]
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known))
}

/// Expands a tmux format for the pane this process runs in; None when tmux
/// cannot answer.
fn tmux_format(format: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", format])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim_end_matches('\n').to_owned())
}

fn stdout_tty() -> Option<String> {
    let mut name = [0; 128];
    // SAFETY: On success ttyname_r leaves a NUL-terminated path in the buffer.
    unsafe {
        (libc::ttyname_r(libc::STDOUT_FILENO, name.as_mut_ptr(), name.len()) == 0)
            .then(|| CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned())
    }
}

fn probe() -> Result<bool> {
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let fd = tty.as_raw_fd();
    // SAFETY: All structures are initialized by libc before use, the file owns
    // fd throughout the guard, and every path restores both tty and signal state.
    let _guard = unsafe {
        let mut original: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut original) != 0 {
            return Ok(false);
        }
        let mut mask: libc::sigset_t = std::mem::zeroed();
        let mut previous: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        for signal in [
            libc::SIGINT,
            libc::SIGTERM,
            libc::SIGHUP,
            libc::SIGQUIT,
            libc::SIGTSTP,
        ] {
            libc::sigaddset(&mut mask, signal);
        }
        if libc::pthread_sigmask(libc::SIG_BLOCK, &mask, &mut previous) != 0 {
            return Ok(false);
        }
        let guard = TerminalGuard {
            fd,
            termios: original,
            signals: previous,
        };
        let mut raw = original;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(fd, libc::TCSANOW, &raw) != 0 {
            return Ok(false);
        }
        guard
    };
    write!(tty, "\x1b_Ga=q,i={QUERY_ID},s=1,v=1,t=d,f=24;AAAA\x1b\\")?;
    tty.flush()?;
    let deadline = Instant::now() + Duration::from_millis(180);
    let mut response = Vec::new();
    while Instant::now() < deadline && response.len() < 4096 {
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(180) as i32;
        if unsafe { libc::poll(&mut poll, 1, timeout) } <= 0 {
            break;
        }
        let mut bytes = [0_u8; 256];
        let count = tty.read(&mut bytes)?;
        response.extend_from_slice(&bytes[..count]);
        if response.ends_with(b"\x1b\\") {
            break;
        }
    }
    Ok(query_succeeded(&response))
}

fn query_succeeded(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    text.contains(&format!("\x1b_Gi={QUERY_ID};OK\x1b\\"))
}

pub fn display(image: &RgbaImage) -> Result<()> {
    // tmux consumes bare graphics commands (as pane titles), so there the
    // image travels through passthrough and is placed with Unicode placeholder
    // cells, which tmux keeps, scrolls and redraws like ordinary text.
    let tmux = std::env::var_os("TMUX").is_some();
    if tmux {
        ensure_passthrough()?;
    }
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe {
        libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size);
    }
    // Placeholder rows and columns are numbered by diacritics, which bounds the grid.
    let limit = if tmux {
        DIACRITICS.len() as u32
    } else {
        u32::MAX
    };
    let cols = u32::from(size.ws_col).max(20).min(limit);
    let rows = u32::from(size.ws_row).max(10).saturating_sub(3).min(limit);
    let cell_width = if size.ws_xpixel > 0 && size.ws_col > 0 {
        (u32::from(size.ws_xpixel) / u32::from(size.ws_col)).max(1)
    } else {
        8
    };
    let cell_height = if size.ws_ypixel > 0 && size.ws_row > 0 {
        (u32::from(size.ws_ypixel) / u32::from(size.ws_row)).max(1)
    } else {
        16
    };
    let (pixel_width, pixel_height) = (cols * cell_width, rows * cell_height);
    let preview = if image.width() > pixel_width || image.height() > pixel_height {
        DynamicImage::ImageRgba8(image.clone())
            .resize(pixel_width, pixel_height, FilterType::Triangle)
            .into_rgba8()
    } else {
        image.clone()
    };
    let span_cols = preview.width().div_ceil(cell_width);
    let span_rows = preview.height().div_ceil(cell_height);
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(preview).write_to(&mut png, ImageFormat::Png)?;
    let mut stdout = std::io::stdout().lock();
    if tmux {
        let id = image_id();
        let control = format!("a=T,U=1,i={id},c={span_cols},r={span_rows},t=d,f=100,q=2");
        write_graphics(&mut stdout, png.get_ref(), &control, true)
            .and_then(|()| write_placeholders(&mut stdout, id, span_cols, span_rows))
    } else {
        write_graphics(&mut stdout, png.get_ref(), "a=T,t=d,f=100,q=2", false)
            .and_then(|()| writeln!(stdout))
    }
    .context("Cannot write image to terminal")?;
    stdout.flush()?;
    Ok(())
}

fn ensure_passthrough() -> Result<()> {
    // A blocked upload would leave only empty placeholder cells. An older tmux
    // without the option always passes through, so only a definite "off" fails.
    ensure!(
        tmux_format("#{allow-passthrough}").as_deref() != Some("off"),
        "tmux blocks terminal images; enable them with `tmux set -g allow-passthrough on`"
    );
    Ok(())
}

/// A per-process random ID. Its low byte travels as a 256-colour foreground,
/// which tmux forwards without RGB support; its nonzero high byte travels as a
/// third diacritic and keeps clear of the small IDs other tools tend to use.
/// The low byte skips indices 0–15: tmux replaces those with the pane palette
/// (`pane-colours`), and a substituted RGB colour names a different image.
fn image_id() -> u32 {
    let [low, high, ..] = RandomState::new()
        .hash_one(std::process::id())
        .to_le_bytes();
    (u32::from(high.max(1)) << 24) | (16 + u32::from(low) % 240)
}

fn write_graphics(
    mut output: impl Write,
    png: &[u8],
    control: &str,
    tmux: bool,
) -> std::io::Result<()> {
    // tmux passthrough wraps each chunk separately and doubles its ESC bytes.
    let (start, escape, end) = if tmux {
        ("\x1bPtmux;", "\x1b\x1b", "\x1b\\")
    } else {
        ("", "\x1b", "")
    };
    let data = STANDARD.encode(png);
    let chunks = data.as_bytes().chunks(4096);
    let total = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        let more = u8::from(index + 1 < total);
        if index == 0 {
            write!(output, "{start}{escape}_G{control},m={more};")?;
        } else {
            write!(output, "{start}{escape}_Gm={more};")?;
        }
        output.write_all(chunk)?;
        write!(output, "{escape}\\{end}")?;
    }
    Ok(())
}

/// Writes one line of placeholder cells per preview row. Each cell names its
/// row and column with diacritics and its image with colour and a third one.
fn write_placeholders(
    mut output: impl Write,
    id: u32,
    columns: u32,
    rows: u32,
) -> std::io::Result<()> {
    let high = DIACRITICS[(id >> 24) as usize];
    for row in DIACRITICS.iter().take(rows as usize) {
        write!(output, "\x1b[38;5;{}m", id & 0xff)?;
        for column in DIACRITICS.iter().take(columns as usize) {
            write!(output, "{PLACEHOLDER}{row}{column}{high}")?;
        }
        writeln!(output, "\x1b[39m")?;
    }
    Ok(())
}

/// Row and column numbers for Unicode placeholders, from kitty's rowcolumn-diacritics.txt.
#[rustfmt::skip]
const DIACRITICS: [char; 297] = [
    '\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}', '\u{0312}', '\u{033D}', '\u{033E}',
    '\u{033F}', '\u{0346}', '\u{034A}', '\u{034B}', '\u{034C}', '\u{0350}', '\u{0351}',
    '\u{0352}', '\u{0357}', '\u{035B}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}',
    '\u{0367}', '\u{0368}', '\u{0369}', '\u{036A}', '\u{036B}', '\u{036C}', '\u{036D}',
    '\u{036E}', '\u{036F}', '\u{0483}', '\u{0484}', '\u{0485}', '\u{0486}', '\u{0487}',
    '\u{0592}', '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}', '\u{0598}', '\u{0599}',
    '\u{059C}', '\u{059D}', '\u{059E}', '\u{059F}', '\u{05A0}', '\u{05A1}', '\u{05A8}',
    '\u{05A9}', '\u{05AB}', '\u{05AC}', '\u{05AF}', '\u{05C4}', '\u{0610}', '\u{0611}',
    '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', '\u{0616}', '\u{0617}', '\u{0657}',
    '\u{0658}', '\u{0659}', '\u{065A}', '\u{065B}', '\u{065D}', '\u{065E}', '\u{06D6}',
    '\u{06D7}', '\u{06D8}', '\u{06D9}', '\u{06DA}', '\u{06DB}', '\u{06DC}', '\u{06DF}',
    '\u{06E0}', '\u{06E1}', '\u{06E2}', '\u{06E4}', '\u{06E7}', '\u{06E8}', '\u{06EB}',
    '\u{06EC}', '\u{0730}', '\u{0732}', '\u{0733}', '\u{0735}', '\u{0736}', '\u{073A}',
    '\u{073D}', '\u{073F}', '\u{0740}', '\u{0741}', '\u{0743}', '\u{0745}', '\u{0747}',
    '\u{0749}', '\u{074A}', '\u{07EB}', '\u{07EC}', '\u{07ED}', '\u{07EE}', '\u{07EF}',
    '\u{07F0}', '\u{07F1}', '\u{07F3}', '\u{0816}', '\u{0817}', '\u{0818}', '\u{0819}',
    '\u{081B}', '\u{081C}', '\u{081D}', '\u{081E}', '\u{081F}', '\u{0820}', '\u{0821}',
    '\u{0822}', '\u{0823}', '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082A}',
    '\u{082B}', '\u{082C}', '\u{082D}', '\u{0951}', '\u{0953}', '\u{0954}', '\u{0F82}',
    '\u{0F83}', '\u{0F86}', '\u{0F87}', '\u{135D}', '\u{135E}', '\u{135F}', '\u{17DD}',
    '\u{193A}', '\u{1A17}', '\u{1A75}', '\u{1A76}', '\u{1A77}', '\u{1A78}', '\u{1A79}',
    '\u{1A7A}', '\u{1A7B}', '\u{1A7C}', '\u{1B6B}', '\u{1B6D}', '\u{1B6E}', '\u{1B6F}',
    '\u{1B70}', '\u{1B71}', '\u{1B72}', '\u{1B73}', '\u{1CD0}', '\u{1CD1}', '\u{1CD2}',
    '\u{1CDA}', '\u{1CDB}', '\u{1CE0}', '\u{1DC0}', '\u{1DC1}', '\u{1DC3}', '\u{1DC4}',
    '\u{1DC5}', '\u{1DC6}', '\u{1DC7}', '\u{1DC8}', '\u{1DC9}', '\u{1DCB}', '\u{1DCC}',
    '\u{1DD1}', '\u{1DD2}', '\u{1DD3}', '\u{1DD4}', '\u{1DD5}', '\u{1DD6}', '\u{1DD7}',
    '\u{1DD8}', '\u{1DD9}', '\u{1DDA}', '\u{1DDB}', '\u{1DDC}', '\u{1DDD}', '\u{1DDE}',
    '\u{1DDF}', '\u{1DE0}', '\u{1DE1}', '\u{1DE2}', '\u{1DE3}', '\u{1DE4}', '\u{1DE5}',
    '\u{1DE6}', '\u{1DFE}', '\u{20D0}', '\u{20D1}', '\u{20D4}', '\u{20D5}', '\u{20D6}',
    '\u{20D7}', '\u{20DB}', '\u{20DC}', '\u{20E1}', '\u{20E7}', '\u{20E9}', '\u{20F0}',
    '\u{2CEF}', '\u{2CF0}', '\u{2CF1}', '\u{2DE0}', '\u{2DE1}', '\u{2DE2}', '\u{2DE3}',
    '\u{2DE4}', '\u{2DE5}', '\u{2DE6}', '\u{2DE7}', '\u{2DE8}', '\u{2DE9}', '\u{2DEA}',
    '\u{2DEB}', '\u{2DEC}', '\u{2DED}', '\u{2DEE}', '\u{2DEF}', '\u{2DF0}', '\u{2DF1}',
    '\u{2DF2}', '\u{2DF3}', '\u{2DF4}', '\u{2DF5}', '\u{2DF6}', '\u{2DF7}', '\u{2DF8}',
    '\u{2DF9}', '\u{2DFA}', '\u{2DFB}', '\u{2DFC}', '\u{2DFD}', '\u{2DFE}', '\u{2DFF}',
    '\u{A66F}', '\u{A67C}', '\u{A67D}', '\u{A6F0}', '\u{A6F1}', '\u{A8E0}', '\u{A8E1}',
    '\u{A8E2}', '\u{A8E3}', '\u{A8E4}', '\u{A8E5}', '\u{A8E6}', '\u{A8E7}', '\u{A8E8}',
    '\u{A8E9}', '\u{A8EA}', '\u{A8EB}', '\u{A8EC}', '\u{A8ED}', '\u{A8EE}', '\u{A8EF}',
    '\u{A8F0}', '\u{A8F1}', '\u{AAB0}', '\u{AAB2}', '\u{AAB3}', '\u{AAB7}', '\u{AAB8}',
    '\u{AABE}', '\u{AABF}', '\u{AAC1}', '\u{FE20}', '\u{FE21}', '\u{FE22}', '\u{FE23}',
    '\u{FE24}', '\u{FE25}', '\u{FE26}', '\u{10A0F}', '\u{10A38}', '\u{1D185}', '\u{1D186}',
    '\u{1D187}', '\u{1D188}', '\u{1D189}', '\u{1D1AA}', '\u{1D1AB}', '\u{1D1AC}', '\u{1D1AD}',
    '\u{1D242}', '\u{1D243}', '\u{1D244}',
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_matching_positive_query_is_accepted() {
        assert!(query_succeeded(b"\x1b_Gi=73921;OK\x1b\\"));
        assert!(!query_succeeded(b"\x1b_Gi=73921;ENOTSUP\x1b\\"));
        assert!(!query_succeeded(b"\x1b_Gi=42;OK\x1b\\"));
    }
    #[test]
    fn tmux_panes_need_passthrough_and_a_placeholder_terminal() {
        let tty = "/dev/ttys003";
        assert!(pane_shows_images("/dev/ttys003\ton\tghostty 1.3.1", tty));
        assert!(pane_shows_images("/dev/ttys003\tall\tkitty(0.42.1)", tty));
        // Passthrough off, or an older tmux without the option.
        assert!(!pane_shows_images("/dev/ttys003\toff\tghostty 1.3.1", tty));
        assert!(!pane_shows_images("/dev/ttys003\t\tghostty 1.3.1", tty));
        // No placeholder support, nested tmux, or no attached client.
        assert!(!pane_shows_images(
            "/dev/ttys003\ton\tWezTerm 20240203",
            tty
        ));
        assert!(!pane_shows_images("/dev/ttys003\ton\ttmux next-3.9", tty));
        assert!(!pane_shows_images("/dev/ttys003\ton\t", tty));
        // A stale TMUX variable resolves to another pane.
        assert!(!pane_shows_images("/dev/ttys009\ton\tghostty 1.3.1", tty));
        assert!(!pane_shows_images("", tty));
    }
    #[test]
    fn graphics_chunks_roundtrip_payload() {
        let data: Vec<u8> = (0..12000).map(|i| (i % 251) as u8).collect();
        let mut encoded = Vec::new();
        write_graphics(&mut encoded, &data, "a=T", false).unwrap();
        let text = String::from_utf8(encoded).unwrap();
        let chunks: Vec<_> = text.split("\x1b\\").filter(|s| !s.is_empty()).collect();
        assert!(chunks.len() > 1);
        assert!(chunks[0].starts_with("\x1b_Ga=T,m=1;"));
        assert!(chunks.last().unwrap().starts_with("\x1b_Gm=0;"));
        let payload = chunks
            .iter()
            .map(|s| s.split_once(';').unwrap().1)
            .collect::<String>();
        assert_eq!(STANDARD.decode(payload).unwrap(), data);
    }
    #[test]
    fn tmux_passthrough_wraps_each_chunk_with_escaped_escapes() {
        let data: Vec<u8> = (0..12000).map(|i| (i % 251) as u8).collect();
        let mut direct = Vec::new();
        write_graphics(&mut direct, &data, "a=T", false).unwrap();
        let mut wrapped = Vec::new();
        write_graphics(&mut wrapped, &data, "a=T", true).unwrap();
        let wrapped = String::from_utf8(wrapped).unwrap();
        let sequences: Vec<_> = wrapped.split("\x1bPtmux;").skip(1).collect();
        assert!(sequences.len() > 1);
        let mut unwrapped = String::new();
        for sequence in sequences {
            let inner = sequence.strip_suffix("\x1b\\").unwrap();
            assert!(!inner.replace("\x1b\x1b", "").contains('\x1b'));
            unwrapped.push_str(&inner.replace("\x1b\x1b", "\x1b"));
        }
        assert!(wrapped.starts_with("\x1bPtmux;"));
        assert_eq!(unwrapped.as_bytes(), direct);
    }
    #[test]
    fn placeholders_number_cells_and_carry_the_image_id() {
        let mut output = Vec::new();
        write_placeholders(&mut output, (3 << 24) | 200, 2, 3).unwrap();
        let text = String::from_utf8(output).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        for (row, line) in lines.iter().enumerate() {
            let cells = line
                .strip_prefix("\x1b[38;5;200m")
                .and_then(|cells| cells.strip_suffix("\x1b[39m"))
                .unwrap();
            let expected: String = (0..2)
                .flat_map(|column| {
                    [
                        PLACEHOLDER,
                        DIACRITICS[row],
                        DIACRITICS[column],
                        DIACRITICS[3],
                    ]
                })
                .collect();
            assert_eq!(cells, expected);
        }
    }
    #[test]
    fn image_ids_fit_the_placeholder_encoding() {
        for _ in 0..64 {
            let id = image_id();
            assert_eq!(id & 0x00ff_ff00, 0);
            assert!((id & 0xff) >= 16, "tmux remaps colour indices 0-15");
            assert_ne!(id >> 24, 0);
        }
    }
}
