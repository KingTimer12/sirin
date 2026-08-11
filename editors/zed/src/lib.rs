use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree};

struct SirinExtension;

impl zed::Extension for SirinExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        // Prefer a `sirin-lsp` already on PATH (e.g. `cargo install --path sirin-lsp`).
        let path = worktree
            .which("sirin-lsp")
            .ok_or_else(|| "`sirin-lsp` not found in PATH. Run `cargo install --path sirin-lsp` in the sirin-lang repo.".to_string())?;

        Ok(Command {
            command: path,
            args: vec![],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(SirinExtension);
