use std::env;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use invert::{
    InversionState, expand_inputs, inversion_state, invert_file, invert_reader_to_writer,
    mime_from_file,
};

#[derive(Debug, Parser)]
#[command(
    name = "invert",
    version,
    about = "Invert file bytes and inspect inverted file signatures."
)]
#[command(subcommand_precedence_over_arg = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    invert: InvertArgs,
}

#[derive(Debug, clap::Args)]
struct InvertArgs {
    /// Files to invert. If omitted, read from standard input.
    #[arg(value_name = "INPUT", num_args = 0..)]
    inputs: Vec<PathBuf>,

    /// Write output to a file. With no value, use the conventional .inv name.
    #[arg(
        short,
        long,
        value_name = "OUTPUT",
        num_args = 0..=1,
        default_missing_value = "__invert_default_output__"
    )]
    output: Option<PathBuf>,

    /// Print each input and output path.
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
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
enum CompletionCommand {
    /// Generate and install a completion file for a shell.
    Install { shell: CompletionShell },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompletionShell {
    Bash,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Mime { file }) => match mime_from_file(&file) {
            Ok(Some(mime)) => {
                println!("{mime}");
                ExitCode::SUCCESS
            }
            Ok(None) => {
                println!("application/octet-stream");
                ExitCode::SUCCESS
            }
            Err(error) => fail(error, 2),
        },
        Some(Command::Is { file }) => match inversion_state(&file) {
            Ok(InversionState::Inverted) => {
                println!("true");
                ExitCode::SUCCESS
            }
            Ok(InversionState::NotInverted) => {
                println!("false");
                ExitCode::from(1)
            }
            Ok(InversionState::Unknown) => {
                println!("unknown");
                ExitCode::from(1)
            }
            Err(error) => fail(error, 2),
        },
        Some(Command::Completions { command }) => match command {
            CompletionCommand::Install { shell } => match install_completion(shell) {
                Ok(path) => {
                    println!("installed bash completion: {}", path.display());
                    ExitCode::SUCCESS
                }
                Err(error) => fail(error, 2),
            },
        },
        None => run_inversion(cli.invert),
    }
}

fn run_inversion(args: InvertArgs) -> ExitCode {
    let use_default_output = args
        .output
        .as_ref()
        .is_some_and(|path| path.as_os_str() == "__invert_default_output__");
    let output = args.output.as_ref().filter(|_| !use_default_output);
    if args.inputs.is_empty() {
        if args.output.is_some() {
            eprintln!("error: --output requires an input filename");
            return ExitCode::from(2);
        }
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut source = stdin.lock();
        let mut target = stdout.lock();
        return match invert_reader_to_writer(&mut source, &mut target) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => fail(error, 1),
        };
    }
    let inputs = match expand_inputs(&args.inputs) {
        Ok(inputs) => inputs,
        Err(error) => return fail(error, 1),
    };
    if output.is_some() && inputs.len() != 1 {
        eprintln!("error: an explicit --output file can be used with exactly one input");
        return ExitCode::from(2);
    }
    if let Some(destination) = output {
        return match invert_file(&inputs[0], Some(destination)) {
            Ok(path) => {
                if args.verbose {
                    eprintln!("inverted {} -> {}", inputs[0].display(), path.display());
                }
                ExitCode::SUCCESS
            }
            Err(error) => fail(error, 1),
        };
    }
    if use_default_output {
        for input in &inputs {
            let path = match invert_file(input, None) {
                Ok(path) => path,
                Err(error) => return fail(error, 1),
            };
            if args.verbose {
                eprintln!("inverted {} -> {}", input.display(), path.display());
            }
        }
        return ExitCode::SUCCESS;
    }

    let stdout = io::stdout();
    let mut target = stdout.lock();
    for input in &inputs {
        let file = match File::open(input) {
            Ok(file) => file,
            Err(error) => return fail(error, 1),
        };
        let mut source = BufReader::new(file);
        if let Err(error) = invert_reader_to_writer(&mut source, &mut target) {
            return fail(error, 1);
        }
    }
    ExitCode::SUCCESS
}

fn install_completion(_shell: CompletionShell) -> io::Result<PathBuf> {
    let data_dir = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "cannot determine home directory")
        })?;
    let directory = data_dir.join("bash-completion/completions");
    fs::create_dir_all(&directory)?;
    let path = directory.join("invert");
    let mut file = File::create(&path)?;
    let mut command = Cli::command();
    generate(Shell::Bash, &mut command, "invert", &mut file);
    Ok(path)
}

fn fail(error: impl std::fmt::Display, code: u8) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::from(code)
}
