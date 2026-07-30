use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use zed_semantic_copy::{
    clipboard::PbcopyClipboard,
    context::CopyContext,
    copy_selection,
    formatter::format_selection,
    install::{self, InstallOptions, UninstallOptions},
};

#[derive(Debug, Parser)]
#[command(
    name = "zed-semantic-copy",
    version,
    about = "Copy an editor selection as a semantic Markdown reference"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Copy the current Zed selection. This is the default command.
    Copy {
        /// Print the formatted reference instead of changing the clipboard.
        #[arg(long)]
        stdout: bool,
        /// Read an editor-neutral copy context as JSON from stdin.
        #[arg(long)]
        stdin_json: bool,
    },
    /// Install the helper plus managed Zed task and keymap entries.
    Install {
        /// Preview the files that would change.
        #[arg(long)]
        dry_run: bool,
        /// Vim-mode key sequence used alongside the fixed cmd-shift-c shortcut.
        #[arg(long, default_value = "space y s")]
        keybinding: String,
        /// Override the Zed configuration directory.
        #[arg(long)]
        config_dir: Option<PathBuf>,
        /// Override the binary installation directory.
        #[arg(long)]
        bin_dir: Option<PathBuf>,
    },
    /// Remove managed Zed entries and the installed helper.
    Uninstall {
        /// Preview the files that would change.
        #[arg(long)]
        dry_run: bool,
        /// Leave the installed helper binary in place.
        #[arg(long)]
        keep_binary: bool,
        /// Override the Zed configuration directory.
        #[arg(long)]
        config_dir: Option<PathBuf>,
        /// Override the binary installation directory.
        #[arg(long)]
        bin_dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(CliCommand::Copy {
        stdout: false,
        stdin_json: false,
    }) {
        CliCommand::Copy { stdout, stdin_json } => {
            let context = if stdin_json {
                CopyContext::from_json_reader(std::io::stdin().lock())?
            } else {
                CopyContext::from_environment()?
            };
            if stdout {
                println!("{}", format_selection(&context));
            } else {
                let mut clipboard = PbcopyClipboard::default();
                copy_selection(&context, &mut clipboard)?;
            }
        }
        CliCommand::Install {
            dry_run,
            keybinding,
            config_dir,
            bin_dir,
        } => {
            let options =
                InstallOptions::for_current_user(config_dir, bin_dir, keybinding, dry_run)?;
            println!("{}", install::install(&options)?);
        }
        CliCommand::Uninstall {
            dry_run,
            keep_binary,
            config_dir,
            bin_dir,
        } => {
            let options =
                UninstallOptions::for_current_user(config_dir, bin_dir, keep_binary, dry_run)?;
            println!("{}", install::uninstall(&options)?);
        }
    }
    Ok(())
}
