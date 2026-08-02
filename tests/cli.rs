use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;

use invert::CHUNK_SIZE;

fn run(args: &[&str], input: &[u8]) -> Output {
    run_in(args, input, None)
}

fn run_in(args: &[&str], input: &[u8], current_dir: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_invert"));
    command.args(args);
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let writer = thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    output
}

fn inverted(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(|byte| byte ^ 0xff).collect()
}

#[test]
fn streams_stdin_to_stdout_across_multiple_chunks() {
    let input = (0..(CHUNK_SIZE * 2 + 17))
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();

    let output = run(&[], &input);

    assert!(output.status.success());
    assert_eq!(output.stdout, inverted(&input));
    assert!(output.stderr.is_empty());
}

#[test]
fn combines_files_and_explicit_stdin_in_argument_order() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.bin");
    let second = directory.path().join("second.bin");
    fs::write(&first, [0x01]).unwrap();
    fs::write(&second, [0x02]).unwrap();

    let output = run(
        &[
            first.to_str().unwrap(),
            "-",
            second.to_str().unwrap(),
            "-o",
            "-",
        ],
        &[0x03],
    );

    assert!(output.status.success());
    assert_eq!(output.stdout, [0xfe, 0xfc, 0xfd]);
}

#[test]
fn writes_stdin_to_a_named_output_file() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("nested/output.bin");

    let output = run(
        &["-o", destination.to_str().unwrap(), "-v"],
        &[0x00, 0x55, 0xff],
    );

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(&destination).unwrap(), [0xff, 0xaa, 0x00]);
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!("inverted <stdin> -> {}\n", destination.display())
    );
}

#[test]
fn writes_relative_input_to_relative_output_in_current_directory() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("input.bin"), [0x00, 0x55, 0xff]).unwrap();

    let output = run_in(
        &["input.bin", "-o", "output.bin"],
        &[],
        Some(directory.path()),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(directory.path().join("output.bin")).unwrap(),
        [0xff, 0xaa, 0x00]
    );
}

#[test]
fn writes_conventional_output_for_relative_input_in_current_directory() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("input.bin"), [0x01, 0x02]).unwrap();

    let output = run_in(&["input.bin", "-o"], &[], Some(directory.path()));

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(directory.path().join("input.bin.inv")).unwrap(),
        [0xfe, 0xfd]
    );
}

#[test]
fn writes_stdin_to_relative_output_in_current_directory() {
    let directory = tempfile::tempdir().unwrap();

    let output = run_in(&["-o", "output.bin"], &[0x0f, 0xf0], Some(directory.path()));

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(directory.path().join("output.bin")).unwrap(),
        [0xf0, 0x0f]
    );
}

#[test]
fn rejects_default_output_for_stdin() {
    let output = run(&["-o"], &[0x00]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        output.stderr,
        b"error: --output without a value requires an input filename\n"
    );
}
