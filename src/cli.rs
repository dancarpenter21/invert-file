use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "invert",
    version,
    about = "Invert file bytes and inspect inverted file signatures."
)]
#[command(subcommand_precedence_over_arg = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub invert: InvertArgs,
}

#[derive(Debug, clap::Args)]
pub struct InvertArgs {
    /// Files to invert, or directories with --recursive. If omitted, read from standard input. Use - for standard input.
    #[arg(value_name = "INPUT", num_args = 0..)]
    pub inputs: Vec<PathBuf>,

    /// Write output to a file. Use - for standard output; with no value, append .inv to the input filename.
    #[arg(
        short,
        long,
        value_name = "OUTPUT",
        num_args = 0..=1,
        default_missing_value = "__invert_default_output__"
    )]
    pub output: Option<PathBuf>,

    /// Recursively invert regular files to conventional sibling output paths.
    #[arg(short, long, conflicts_with = "output")]
    pub recursive: bool,

    /// Print each input and output path.
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the magic-byte MIME type of a regular or inverted file.
    Mime { file: PathBuf },
    /// Report whether magic bytes identify a file as inverted.
    Is { file: PathBuf },
    /// Install shell completion support.
    Completions {
        #[command(subcommand)]
        command: CompletionCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum CompletionCommand {
    /// Generate and install a completion file for a shell.
    Install { shell: CompletionShell },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CompletionShell {
    Bash,
}
