use std::env;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
use invert::{
    InversionState, expand_inputs, inversion_state, invert_file, invert_reader_to_file,
    invert_reader_to_writer, mime_from_file,
};
use invert::cli::{Cli, Command, CompletionCommand, CompletionShell, InvertArgs};

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
    let output_is_stdout = output.is_some_and(|path| path.as_os_str() == "-");
    let inputs = match expand_inputs(&args.inputs) {
        Ok(inputs) => inputs,
        Err(error) => return fail(error, 1),
    };
    if use_default_output && inputs.is_empty() {
        eprintln!("error: --output without a value requires an input filename");
        return ExitCode::from(2);
    }
    if output.is_some() && !output_is_stdout && inputs.len() > 1 {
        eprintln!("error: an explicit --output file can be used with exactly one input");
        return ExitCode::from(2);
    }
    if let Some(destination) = output.filter(|_| !output_is_stdout) {
        return invert_to_named_output(inputs.first(), destination, args.verbose);
    }
    if use_default_output {
        if inputs.iter().any(|input| is_stdin(input)) {
            eprintln!("error: --output without a value requires an input filename");
            return ExitCode::from(2);
        }
        for input in &inputs {
            let path = match invert_file(input, None) {
                Ok(path) => path,
                Err(error) => return fail(error, 1),
            };
            report_inversion(args.verbose, input, &path);
        }
        return ExitCode::SUCCESS;
    }

    invert_to_stdout(&inputs, args.verbose)
}

fn invert_to_named_output(input: Option<&PathBuf>, destination: &Path, verbose: bool) -> ExitCode {
    match input {
        Some(input) if !is_stdin(input) => match invert_file(input, Some(destination)) {
            Ok(path) => {
                report_inversion(verbose, input, &path);
                ExitCode::SUCCESS
            }
            Err(error) => fail(error, 1),
        },
        _ => {
            let stdin = io::stdin();
            let mut source = stdin.lock();
            match invert_reader_to_file(&mut source, destination) {
                Ok(path) => {
                    report_inversion(verbose, Path::new("-"), &path);
                    ExitCode::SUCCESS
                }
                Err(error) => fail(error, 1),
            }
        }
    }
}

fn invert_to_stdout(inputs: &[PathBuf], verbose: bool) -> ExitCode {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let stdout = io::stdout();
    let mut target = stdout.lock();
    if inputs.is_empty() {
        return finish_stdout_inversion(
            invert_reader_to_writer(&mut stdin, &mut target),
            verbose,
            Path::new("-"),
        );
    }

    for input in inputs {
        let result = if is_stdin(input) {
            invert_reader_to_writer(&mut stdin, &mut target)
        } else {
            let file = match File::open(input) {
                Ok(file) => file,
                Err(error) => return fail(error, 1),
            };
            let mut source = BufReader::new(file);
            invert_reader_to_writer(&mut source, &mut target)
        };
        match finish_stdout_inversion(result, verbose, input) {
            ExitCode::SUCCESS => {}
            status => return status,
        }
    }
    ExitCode::SUCCESS
}

fn finish_stdout_inversion(result: io::Result<()>, verbose: bool, input: &Path) -> ExitCode {
    match result {
        Ok(()) => {
            report_inversion(verbose, input, Path::new("-"));
            ExitCode::SUCCESS
        }
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => fail(error, 1),
    }
}

fn is_stdin(path: &Path) -> bool {
    path.as_os_str() == "-"
}

fn report_inversion(verbose: bool, input: &Path, output: &Path) {
    if verbose {
        let input = if is_stdin(input) {
            "<stdin>".to_owned()
        } else {
            input.display().to_string()
        };
        let output = if is_stdin(output) {
            "<stdout>".to_owned()
        } else {
            output.display().to_string()
        };
        eprintln!("inverted {input} -> {output}");
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_stdout_pipe_is_successful() {
        let result = finish_stdout_inversion(
            Err(io::Error::from(io::ErrorKind::BrokenPipe)),
            false,
            Path::new("-"),
        );

        assert_eq!(result, ExitCode::SUCCESS);
    }
}
