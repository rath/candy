use anyhow::{Context, Result, bail, ensure};
use candy_core::{Operation, PixelRect};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{
    io::IsTerminal,
    path::PathBuf,
    process::{Command, Stdio},
};

#[derive(Parser)]
#[command(
    name = "candy",
    version,
    about = "A little image tool for your Mac",
    subcommand_negates_reqs = true
)]
pub struct Cli {
    /// Image to display; omit to open the editor.
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
    /// Resize; one dimension preserves the aspect ratio.
    Resize {
        #[command(flatten)]
        files: Files,
        #[arg(long, required_unless_present = "height")]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
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
    #[command(name = "__gui", hide = true)]
    Gui { path: Option<PathBuf> },
}
#[derive(Args)]
pub struct Files {
    pub path: PathBuf,
    #[arg(short, long)]
    pub output: PathBuf,
    /// Replace an existing output file (never the input).
    #[arg(long)]
    pub force: bool,
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Action::Gui { path }) => crate::gui::run(path),
        Some(Action::Edit { path }) => launch(Some(path)),
        Some(Action::Resize {
            files,
            width,
            height,
        }) => edit(files, |image| {
            let (width, height) =
                candy_core::resize_dimensions(image.width(), image.height(), width, height)?;
            Ok(Operation::Resize { width, height })
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
                regions.into_iter().map(|r| r.rect).collect()
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
        None => {
            let Some(path) = cli.path else {
                return crate::gui::run(None);
            };
            if matches!(cli.display, Display::Gui) {
                return launch(Some(path));
            }
            ensure!(
                std::io::stdout().is_terminal(),
                "Image display needs a terminal; use --display gui to open the editor"
            );
            let image = candy_macos::load(&path)?;
            if matches!(cli.display, Display::Kitty) || crate::terminal::supported() {
                crate::terminal::display(&image)
            } else {
                launch(Some(path))
            }
        }
    }
}

fn edit(
    files: Files,
    operation: impl FnOnce(&image::RgbaImage) -> Result<Operation>,
) -> Result<()> {
    candy_core::protect_original(&files.path, &files.output)?;
    ensure!(
        files.force || !files.output.exists(),
        "Output already exists; use --force to replace it"
    );
    let image = candy_macos::load(&files.path)?;
    let result = candy_core::apply(&image, &operation(&image)?)?;
    candy_core::save_atomic(&result, &files.output, files.force)?;
    eprintln!(
        "Saved {} ({} × {})",
        files.output.display(),
        result.width(),
        result.height()
    );
    Ok(())
}

fn launch(path: Option<PathBuf>) -> Result<()> {
    let path = path
        .map(|p| {
            p.canonicalize()
                .with_context(|| format!("Cannot open {}", p.display()))
        })
        .transpose()?;
    if path.as_ref().is_some_and(|p| !p.is_file()) {
        bail!("Choose an image file");
    }
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("__gui");
    if let Some(path) = path {
        command.arg(path);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    // A new session survives the originating terminal closing.
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().context("Cannot launch the Candy editor")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
