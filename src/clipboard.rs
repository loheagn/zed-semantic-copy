use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context as _, Result, bail};

pub trait ClipboardSink {
    fn write(&mut self, text: &str) -> Result<()>;
}

#[derive(Clone, Debug)]
pub struct PbcopyClipboard {
    program: PathBuf,
}

impl Default for PbcopyClipboard {
    fn default() -> Self {
        Self::new("/usr/bin/pbcopy")
    }
}

impl PbcopyClipboard {
    pub fn new(program: impl AsRef<Path>) -> Self {
        Self {
            program: program.as_ref().to_path_buf(),
        }
    }
}

impl ClipboardSink for PbcopyClipboard {
    fn write(&mut self, text: &str) -> Result<()> {
        let mut child = Command::new(&self.program)
            // GUI-launched Zed processes may not have a locale. In that case
            // pbcopy falls back to the C encoding and silently drops the
            // Chinese text used by semantic references.
            .env_remove("LC_ALL")
            .env("LC_CTYPE", "UTF-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to launch {}", self.program.display()))?;

        child
            .stdin
            .take()
            .context("pbcopy stdin was not available")?
            .write_all(text.as_bytes())
            .context("failed to write the semantic reference to pbcopy")?;

        let output = child
            .wait_with_output()
            .context("failed while waiting for pbcopy")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("pbcopy failed: {}", stderr.trim());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::{fs, os::unix::fs::PermissionsExt as _};

    use super::*;

    #[test]
    fn missing_pbcopy_is_reported() {
        let mut clipboard = PbcopyClipboard::new("/definitely/missing/pbcopy");
        assert!(clipboard.write("hello").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn passes_exact_utf8_bytes_to_pbcopy_stdin() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("capture-stdin");
        let captured = directory.path().join("captured");
        let locale = directory.path().join("locale");
        fs::write(
            &program,
            "#!/bin/sh\ndir=${0%/*}\nprintf '%s' \"$LC_CTYPE\" > \"$dir/locale\"\n/bin/cat > \"$dir/captured\"\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        let text = "中文 $() `ticks` without a trailing newline";

        PbcopyClipboard::new(&program).write(text).unwrap();

        assert_eq!(fs::read(captured).unwrap(), text.as_bytes());
        assert_eq!(fs::read_to_string(locale).unwrap(), "UTF-8");
    }

    #[cfg(unix)]
    #[test]
    fn reports_pbcopy_stderr_on_nonzero_exit() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("failing-pbcopy");
        fs::write(&program, "#!/bin/sh\necho clipboard-denied >&2\nexit 7\n").unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();

        let error = PbcopyClipboard::new(&program).write("text").unwrap_err();

        assert!(error.to_string().contains("clipboard-denied"));
    }
}
