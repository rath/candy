use crate::launcher::launch;
use anyhow::{Context, Result, ensure};
use candy_core::{Operation, PixelRect, Rotation, SaveOptions, WebpMode};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{
    io::{IsTerminal, Read},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    name = "candy",
    version,
    about = "A little image tool for your Mac",
    subcommand_negates_reqs = true
)]
pub struct Cli {
    /// Image to display, or - for stdin. Omit to read piped input or open the editor.
    pub path: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "auto")]
    pub display: Display,
    #[command(subcommand)]
    pub command: Option<Action>,
}
#[derive(Clone, Copy, ValueEnum)]
pub enum Display {
    Auto,
    Kitty,
    Gui,
}
#[derive(Subcommand)]
pub enum Action {
    /// Open an image in the GUI editor and return to the shell.
    Edit { path: PathBuf },
    /// Show an image's format, dimensions, color and pixel statistics.
    Inspect {
        path: PathBuf,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Resize; one dimension preserves the aspect ratio.
    Resize {
        #[command(flatten)]
        files: Files,
        #[arg(long, required_unless_present = "height")]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
    },
    /// Rotate by 90 degrees.
    Rotate {
        #[command(flatten)]
        files: Files,
        /// Turn clockwise (to the right).
        #[arg(long, required_unless_present = "ccw", conflicts_with = "ccw")]
        cw: bool,
        /// Turn counterclockwise (to the left).
        #[arg(long)]
        ccw: bool,
    },
    /// Extract a pixel rectangle into a new image.
    Crop {
        #[command(flatten)]
        files: Files,
        #[arg(long)]
        rect: PixelRect,
    },
    /// Pixelate a rectangle or all detected text.
    Mosaic {
        #[command(flatten)]
        files: Files,
        #[arg(long, required_unless_present = "text", conflicts_with = "text")]
        rect: Option<PixelRect>,
        #[arg(long)]
        text: bool,
        #[arg(long, default_value_t = 12, value_parser = clap::value_parser!(u32).range(2..))]
        block_size: u32,
    },
    /// Composite transparency onto a solid background.
    Flatten {
        #[command(flatten)]
        files: Files,
        #[arg(long, default_value = "#ffffff", value_parser = candy_core::parse_background)]
        background: [u8; 3],
    },
}
#[derive(Args)]
pub struct Files {
    /// Image to edit.
    pub path: PathBuf,
    /// Write the result here and leave the input untouched.
    #[arg(
        short,
        long,
        required_unless_present = "in_place",
        conflicts_with = "in_place"
    )]
    pub output: Option<PathBuf>,
    /// Replace the input file itself, re-encoded in its own format.
    #[arg(short, long)]
    pub in_place: bool,
    /// Replace an existing output file (never the input).
    #[arg(long, conflicts_with = "in_place")]
    pub force: bool,
    /// Output quality (JPEG, lossy WebP, HEIC, AVIF or JPEG 2000).
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=100))]
    pub quality: Option<u8>,
    /// WebP compression (default: lossless); --quality requires lossy.
    #[arg(long, value_enum)]
    pub webp_mode: Option<WebpCompression>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum WebpCompression {
    Lossless,
    Lossy,
}

impl Files {
    fn save_options(&self) -> SaveOptions {
        SaveOptions {
            quality: self.quality,
            webp_mode: self.webp_mode.map(|mode| match mode {
                WebpCompression::Lossless => WebpMode::Lossless,
                WebpCompression::Lossy => WebpMode::Lossy,
            }),
        }
    }
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Action::Edit { path }) => launch(Some(path)),
        Some(Action::Inspect { path, json }) => crate::inspect::run(&path, json),
        Some(Action::Resize {
            files,
            width,
            height,
        }) => edit(files, |image| {
            let (width, height) =
                candy_core::resize_dimensions(image.width(), image.height(), width, height)?;
            Ok(Operation::Resize { width, height })
        }),
        Some(Action::Rotate { files, cw, .. }) => edit(files, |_| {
            Ok(Operation::Rotate(if cw {
                Rotation::Clockwise
            } else {
                Rotation::CounterClockwise
            }))
        }),
        Some(Action::Crop { files, rect }) => edit(files, |_| Ok(Operation::Crop(rect))),
        Some(Action::Mosaic {
            files,
            rect,
            text,
            block_size,
        }) => edit(files, |image| {
            let regions = if text {
                let regions = candy_macos::detect_text(image)?;
                eprintln!("Detected {} text regions", regions.len());
                regions
            } else {
                vec![rect.context("Specify --rect or --text")?]
            };
            Ok(Operation::Mosaic {
                regions,
                block: block_size,
            })
        }),
        Some(Action::Flatten { files, background }) => {
            edit(files, |_| Ok(Operation::Flatten(background)))
        }
        None => view(cli.path, cli.display),
    }
}

#[derive(Debug, PartialEq)]
enum Input {
    Editor,
    File(PathBuf),
    Stdin,
}

impl Input {
    fn select(path: Option<PathBuf>, stdin_is_terminal: bool) -> Self {
        match path {
            Some(path) if path.as_os_str() == "-" => Self::Stdin,
            Some(path) => Self::File(path),
            None if stdin_is_terminal => Self::Editor,
            None => Self::Stdin,
        }
    }
}

fn view(path: Option<PathBuf>, display: Display) -> Result<()> {
    let input = Input::select(path, std::io::stdin().is_terminal());
    if input == Input::Editor {
        return launch(None);
    }
    if matches!(display, Display::Gui) {
        if let Input::File(path) = input {
            return launch(Some(path));
        }
    } else {
        ensure!(
            std::io::stdout().is_terminal(),
            "Image display needs a terminal; use --display gui to open the editor"
        );
    }
    let image = match &input {
        Input::File(path) => candy_macos::load(path)?,
        Input::Stdin => read_stdin(std::io::stdin().lock())?,
        Input::Editor => unreachable!(),
    };
    if matches!(display, Display::Kitty)
        || (matches!(display, Display::Auto) && crate::terminal::supported())
    {
        crate::terminal::display(&image)
    } else if let Input::File(path) = input {
        launch(Some(path))
    } else {
        crate::launcher::launch_image(&image)
    }
}

fn read_stdin(input: impl Read) -> Result<image::RgbaImage> {
    let bytes = candy_core::input::read_limited(input, candy_core::input::MAX_INPUT_BYTES)
        .context("Cannot read image from stdin")?;
    ensure!(!bytes.is_empty(), "No image data received on stdin");
    candy_macos::load_bytes(&bytes).context("Cannot decode image from stdin")
}

fn edit(
    files: Files,
    operation: impl FnOnce(&image::RgbaImage) -> Result<Operation>,
) -> Result<()> {
    let (output, overwrite) = match files.output.clone() {
        Some(output) => {
            candy_core::protect_original(&files.path, &output)?;
            ensure!(
                files.force || !output.exists(),
                "Output already exists; use --force to replace it"
            );
            (output, files.force)
        }
        // Resolve aliases so a symlink keeps pointing at the edited file
        // instead of being replaced by a regular file.
        None => (
            files
                .path
                .canonicalize()
                .context("Cannot resolve the image to edit in place")?,
            true,
        ),
    };
    let options = files.save_options();
    options.validate(&candy_core::extension(&output))?;
    let image = candy_macos::load(&files.path)?;
    let result = candy_core::apply(&image, &operation(&image)?)?;
    candy_macos::save_with_options(&result, &output, overwrite, options)?;
    eprintln!(
        "Saved {} ({} × {})",
        files.output.as_deref().unwrap_or(&files.path).display(),
        result.width(),
        result.height()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_selects_stdin_only_when_requested_or_redirected() {
        assert_eq!(Input::select(None, true), Input::Editor);
        assert_eq!(Input::select(None, false), Input::Stdin);
        for terminal in [false, true] {
            assert_eq!(Input::select(Some("-".into()), terminal), Input::Stdin);
            for path in ["photo.png", "./-"] {
                assert_eq!(
                    Input::select(Some(path.into()), terminal),
                    Input::File(path.into())
                );
            }
        }
        assert_eq!(
            Cli::try_parse_from(["candy", "-"]).unwrap().path,
            Some("-".into())
        );
    }

    #[test]
    fn stdin_errors_identify_empty_invalid_and_failed_reads() {
        assert!(
            read_stdin(&b""[..])
                .unwrap_err()
                .to_string()
                .contains("No image data")
        );
        assert!(
            read_stdin(&b"not an image"[..])
                .unwrap_err()
                .to_string()
                .contains("decode image from stdin")
        );
        struct Failed;
        impl Read for Failed {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("read failed"))
            }
        }
        assert!(
            read_stdin(Failed)
                .unwrap_err()
                .to_string()
                .contains("read image from stdin")
        );
    }

    #[test]
    fn cli_rejects_missing_outputs_and_conflicting_regions() {
        assert!(Cli::try_parse_from(["candy", "resize", "a.png", "--width", "10"]).is_err());
        assert!(
            Cli::try_parse_from([
                "candy",
                "mosaic",
                "a.png",
                "-o",
                "b.png",
                "--text",
                "--rect",
                "0,0,10,10"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from(["candy", "crop", "a.png", "-o", "b.png", "--rect", "0,0,0,2"])
                .is_err()
        );
        assert!(
            Cli::try_parse_from(["candy", "resize", "a.png", "-o", "b.png", "--width", "10"])
                .is_ok()
        );
    }
    #[test]
    fn edits_write_to_exactly_one_destination() {
        let parse =
            |args: &[&str]| Cli::try_parse_from([&["candy", "crop", "a.png"], args].concat());
        assert!(parse(&["--rect", "0,0,1,1"]).is_err());
        assert!(parse(&["--rect", "0,0,1,1", "-o", "b.png", "-i"]).is_err());
        assert!(parse(&["--rect", "0,0,1,1", "-i", "--force"]).is_err());
        assert!(matches!(
            parse(&["--rect", "0,0,1,1", "--in-place"]).unwrap().command,
            Some(Action::Crop {
                files: Files {
                    output: None,
                    in_place: true,
                    force: false,
                    ..
                },
                ..
            })
        ));
        assert!(matches!(
            parse(&["--rect", "0,0,1,1", "-o", "b.png", "--force"])
                .unwrap()
                .command,
            Some(Action::Crop {
                files: Files {
                    output: Some(_),
                    in_place: false,
                    force: true,
                    ..
                },
                ..
            })
        ));
    }
    #[test]
    fn rotate_takes_exactly_one_direction() {
        assert!(Cli::try_parse_from(["candy", "rotate", "a.png", "-o", "b.png"]).is_err());
        assert!(
            Cli::try_parse_from(["candy", "rotate", "a.png", "-o", "b.png", "--cw", "--ccw"])
                .is_err()
        );
        for flag in ["--cw", "--ccw"] {
            assert!(matches!(
                Cli::try_parse_from(["candy", "rotate", "a.png", "-o", "b.png", flag])
                    .unwrap()
                    .command,
                Some(Action::Rotate { cw, ccw, .. }) if cw == (flag == "--cw") && ccw != cw
            ));
        }
    }
    #[test]
    fn inspect_takes_a_path_and_an_optional_json_flag() {
        assert!(Cli::try_parse_from(["candy", "inspect"]).is_err());
        assert!(Cli::try_parse_from(["candy", "inspect", "a.png"]).is_ok());
        assert!(matches!(
            Cli::try_parse_from(["candy", "inspect", "a.png", "--json"])
                .unwrap()
                .command,
            Some(Action::Inspect { json: true, .. })
        ));
    }
}
