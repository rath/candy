//! Launch the separately built native application through LaunchServices.
use anyhow::{Context, Result, bail, ensure};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

const BUNDLE_ID: &str = "org.candyapp.Candy";

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
    if let Some(path) = path {
        command.arg(path);
    }
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
