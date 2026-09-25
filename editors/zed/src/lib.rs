use std::fs;

use zed_extension_api::{
    self as zed, Architecture, Command, DownloadedFileType, GithubReleaseOptions,
    LanguageServerId, LanguageServerInstallationStatus, Os, Result, Worktree,
};

/// GitHub repository whose releases carry prebuilt `sirin-lsp` archives
/// (built by `.github/workflows/release.yml`).
const REPO: &str = "KingTimer12/sirin";

struct SirinExtension {
    cached_binary_path: Option<String>,
}

impl SirinExtension {
    fn language_server_binary_path(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<String> {
        // A `sirin-lsp` on PATH wins, so local builds (`cargo install --path sirin-lsp`)
        // can be tested without publishing a release.
        if let Some(path) = worktree.which("sirin-lsp") {
            return Ok(path);
        }
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).is_ok_and(|m| m.is_file()) {
                return Ok(path.clone());
            }
        }

        zed::set_language_server_installation_status(
            id,
            &LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = zed::latest_github_release(
            REPO,
            GithubReleaseOptions { require_assets: true, pre_release: false },
        )
        .map_err(|e| {
            format!(
                "could not find a sirin-lsp release on GitHub ({e}); install it with \
                 `cargo install --path sirin-lsp` from the sirin repo and make sure it is on PATH"
            )
        })?;

        let (os, arch) = zed::current_platform();
        let target = match (os, arch) {
            (Os::Linux, Architecture::X8664) => "x86_64-unknown-linux-gnu",
            (Os::Mac, Architecture::X8664) => "x86_64-apple-darwin",
            (Os::Mac, Architecture::Aarch64) => "aarch64-apple-darwin",
            (Os::Windows, Architecture::X8664) => "x86_64-pc-windows-msvc",
            _ => {
                return Err(format!(
                    "no prebuilt sirin-lsp for this platform ({os:?}/{arch:?}); \
                     build it with `cargo install --path sirin-lsp`"
                ));
            }
        };
        let (ext, file_type, exe) = match os {
            Os::Windows => ("zip", DownloadedFileType::Zip, ".exe"),
            _ => ("tar.gz", DownloadedFileType::GzipTar, ""),
        };
        let asset_name = format!("sirin-lsp-{target}.{ext}");
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| format!("release {} has no asset `{asset_name}`", release.version))?;

        let version_dir = format!("sirin-lsp-{}", release.version);
        let binary_path = format!("{version_dir}/sirin-lsp{exe}");
        if !fs::metadata(&binary_path).is_ok_and(|m| m.is_file()) {
            zed::set_language_server_installation_status(
                id,
                &LanguageServerInstallationStatus::Downloading,
            );
            zed::download_file(&asset.download_url, &version_dir, file_type)
                .map_err(|e| format!("failed to download `{asset_name}`: {e}"))?;
            zed::make_file_executable(&binary_path)?;

            // Drop older downloaded versions.
            if let Ok(entries) = fs::read_dir(".") {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with("sirin-lsp-") && name != version_dir {
                        let _ = fs::remove_dir_all(entry.path());
                    }
                }
            }
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl zed::Extension for SirinExtension {
    fn new() -> Self {
        Self { cached_binary_path: None }
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        Ok(Command {
            command: self.language_server_binary_path(id, worktree)?,
            args: vec![],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(SirinExtension);
