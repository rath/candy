//! Launch the separately built native application through LaunchServices.
use anyhow::{Context, Result, bail, ensure};
use std::{
    fs::{self, DirBuilder},
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

const BUNDLE_ID: &str = "me.told.candy.app";

/// LaunchServices opens documents asynchronously. Transfer ownership of this
/// private PNG to the app only after `open` succeeds; the app removes it after
/// decoding. Keep this layout in sync with CandyKit's CLIImport.
pub fn launch_image(image: &image::RgbaImage) -> Result<()> {
    let mut command = open_command()?;
    let root = std::env::home_dir()
        .context("Cannot find the user cache directory")?
        .join("Library/Caches/me.told.candy.app/stdin");
    handoff_image(image, &root, |path| {
        command.arg(path);
        run_open(command)
    })
}

fn handoff_image(
    image: &image::RgbaImage,
    root: &Path,
    launch: impl FnOnce(PathBuf) -> Result<()>,
) -> Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(root)
        .context("Cannot create the stdin image cache")?;
    let metadata = fs::symlink_metadata(root)?;
    ensure!(
        metadata.is_dir()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0,
        "The stdin image cache must be a private directory owned by the current user"
    );
    let directory = tempfile::Builder::new()
        .prefix("import-")
        .rand_bytes(16)
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(root)?;
    let path = directory.path().join("stdin.png");
    candy_core::save_atomic(image, &path, false)
        .context("Cannot prepare the stdin image for the editor")?;
    launch(path)?;
    let _ = directory.keep();
    Ok(())
}

fn valid_app(path: &Path) -> bool {
    path.join("Contents/Info.plist").is_file() && path.join("Contents/MacOS/Candy").is_file()
}

fn resolve_app(explicit: Option<PathBuf>, executable: &Path) -> Result<Option<PathBuf>> {
    if let Some(path) = explicit {
        ensure!(
            valid_app(&path),
            "CANDY_APP must point to a built Candy.app: {}",
            path.display()
        );
        return Ok(Some(path));
    }
    for parent in executable.ancestors().skip(1).take(5) {
        if parent.extension().is_some_and(|ext| ext == "app") && valid_app(parent) {
            return Ok(Some(parent.to_owned()));
        }
        let sibling = parent.join("Candy.app");
        if valid_app(&sibling) {
            return Ok(Some(sibling));
        }
        if parent.join("Cargo.toml").is_file() {
            let local = parent.join("dist/Candy.app");
            if valid_app(&local) {
                return Ok(Some(local));
            }
        }
    }
    Ok(None)
}

pub fn launch(path: Option<PathBuf>) -> Result<()> {
    let path = path
        .map(|p| {
            p.canonicalize()
                .with_context(|| format!("Cannot open {}", p.display()))
        })
        .transpose()?;
    if let Some(path) = &path {
        ensure!(path.is_file(), "Choose an image file");
    }
    let mut command = open_command()?;
    if let Some(path) = path {
        command.arg(path);
    }
    run_open(command)
}

fn open_command() -> Result<Command> {
    let application = resolve_app(
        std::env::var_os("CANDY_APP").map(PathBuf::from),
        &std::env::current_exe()?,
    )?;
    let mut command = Command::new("/usr/bin/open");
    if let Some(app) = application {
        command.arg("-a").arg(app);
    } else {
        command.arg("-b").arg(BUNDLE_ID);
    }
    Ok(command)
}

fn run_open(mut command: Command) -> Result<()> {
    let status = command
        .status()
        .context("Cannot contact macOS LaunchServices")?;
    if !status.success() {
        bail!(
            "Cannot launch Candy.app. Build it with scripts/build-app.sh, install it in Applications, or set CANDY_APP to its path. cargo install installs only the CLI."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stdin_handoff_keeps_full_image_only_after_successful_launch() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("stdin");
        let image = image::RgbaImage::from_pixel(4100, 2, image::Rgba([255, 0, 0, 128]));
        let mut handed = None;
        handoff_image(&image, &root, |path| {
            assert_eq!(candy_macos::load(&path)?.dimensions(), (4100, 2));
            assert_eq!(fs::metadata(&path)?.mode() & 0o077, 0);
            let parent = path.parent().unwrap();
            assert_eq!(fs::metadata(parent)?.mode() & 0o077, 0);
            assert_eq!(parent.file_name().unwrap().len(), 23);
            handed = Some(path);
            Ok(())
        })
        .unwrap();
        assert!(handed.unwrap().is_file());
        let before = fs::read_dir(&root).unwrap().count();
        let failed = handoff_image(&image, &root, |_| anyhow::bail!("launch failed"));
        assert!(failed.is_err());
        assert_eq!(fs::read_dir(&root).unwrap().count(), before);
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        assert!(handoff_image(&image, &alias, |_| panic!("must not launch")).is_err());
    }

    fn app(path: &Path) {
        std::fs::create_dir_all(path.join("Contents/MacOS")).unwrap();
        std::fs::write(path.join("Contents/Info.plist"), "test").unwrap();
        std::fs::write(path.join("Contents/MacOS/Candy"), "test").unwrap();
    }
    #[test]
    fn explicit_app_is_checked_and_takes_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("한글 Candy.app");
        app(&path);
        assert_eq!(
            resolve_app(Some(path.clone()), Path::new("/bin/candy")).unwrap(),
            Some(path)
        );
        assert!(
            resolve_app(
                Some(dir.path().join("missing.app")),
                Path::new("/bin/candy")
            )
            .is_err()
        );
    }
    #[test]
    fn discovers_bundle_and_repository_build_without_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dist/Candy.app");
        app(&path);
        let bundled = path.join("Contents/Helpers/candy");
        assert_eq!(resolve_app(None, &bundled).unwrap(), Some(path.clone()));
        std::fs::write(dir.path().join("Cargo.toml"), "test").unwrap();
        let development = dir.path().join("target/debug/candy");
        assert_eq!(resolve_app(None, &development).unwrap(), Some(path));
        assert_eq!(
            resolve_app(None, Path::new("/nonexistent/bin/candy")).unwrap(),
            None
        );
    }
}
