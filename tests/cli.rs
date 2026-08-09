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
fn conventional_output_restores_the_name_without_an_inv_suffix() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("input.bin");
    let inverted_path = directory.path().join("input.bin.inv");
    let contents = [0x01, 0x02, 0x80, 0xff];
    fs::write(&original, contents).unwrap();

    let first = run_in(&["input.bin", "-o"], &[], Some(directory.path()));
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(fs::read(&inverted_path).unwrap(), inverted(&contents));

    let second = run_in(&["input.bin.inv", "-o"], &[], Some(directory.path()));
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(fs::read(&original).unwrap(), contents);
    assert!(!directory.path().join("input.bin.inv.inv").exists());
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

#[test]
fn recursively_writes_sibling_outputs_for_nested_and_hidden_files() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("tree");
    fs::create_dir_all(tree.join("nested")).unwrap();
    fs::write(tree.join("root.bin"), [0x00, 0x55]).unwrap();
    fs::write(tree.join("nested/.hidden"), [0xff, 0x0f]).unwrap();

    let output = run_in(&["-r", "tree"], &[], Some(directory.path()));

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(tree.join("root.bin.inv")).unwrap(), [0xff, 0xaa]);
    assert_eq!(
        fs::read(tree.join("nested/.hidden.inv")).unwrap(),
        [0x00, 0xf0]
    );
    assert!(!tree.join("root.bin.inv.inv").exists());
}

#[cfg(unix)]
#[test]
fn recursive_mode_skips_symbolic_links() {
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;

    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("tree");
    let linked_directory = directory.path().join("linked-directory");
    fs::create_dir_all(&tree).unwrap();
    fs::create_dir_all(&linked_directory).unwrap();
    fs::write(tree.join("target.bin"), [0x01]).unwrap();
    fs::write(linked_directory.join("outside.bin"), [0x02]).unwrap();
    symlink("target.bin", tree.join("file-link")).unwrap();
    symlink(&linked_directory, tree.join("directory-link")).unwrap();
    let _socket = UnixListener::bind(tree.join("socket")).unwrap();

    let output = run(&["-r", tree.to_str().unwrap()], &[]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tree.join("target.bin.inv").exists());
    assert!(!tree.join("file-link.inv").exists());
    assert!(!tree.join("socket.inv").exists());
    assert!(!linked_directory.join("outside.bin.inv").exists());
}

#[test]
fn recursive_mode_deduplicates_overlapping_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("tree");
    fs::create_dir_all(&tree).unwrap();
    fs::write(tree.join("file.bin"), [0x01]).unwrap();

    let output = run_in(
        &["-r", "-v", "tree", "tree/file.bin"],
        &[],
        Some(directory.path()),
    );

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "inverted tree/file.bin -> tree/file.bin.inv\n"
    );
}

#[test]
fn recursive_mode_rejects_input_output_collisions_before_writing() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("tree");
    fs::create_dir_all(&tree).unwrap();
    let original = [0x01, 0x02];
    let existing_output = [0x10, 0x20];
    fs::write(tree.join("file.bin"), original).unwrap();
    fs::write(tree.join("file.bin.inv"), existing_output).unwrap();

    let output = run(&["--recursive", tree.to_str().unwrap()], &[]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("recursive output is also a selected input")
    );
    assert_eq!(fs::read(tree.join("file.bin")).unwrap(), original);
    assert_eq!(
        fs::read(tree.join("file.bin.inv")).unwrap(),
        existing_output
    );
}

#[test]
fn recursive_mode_rejects_output_options_and_standard_input() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("tree");
    fs::create_dir_all(&tree).unwrap();

    for args in [
        vec!["-r", "-o", tree.to_str().unwrap()],
        vec!["-or", tree.to_str().unwrap()],
        vec!["-ro", tree.to_str().unwrap()],
    ] {
        let output = run(&args, &[]);
        assert_eq!(output.status.code(), Some(2), "arguments: {args:?}");
    }

    let output = run(&["-r", "-"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        output.stderr,
        b"error: --recursive cannot be used with standard input\n"
    );

    let output = run(&["-r"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        output.stderr,
        b"error: --recursive requires at least one input path\n"
    );
}

#[test]
fn recursive_mode_accepts_an_empty_directory() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("empty")).unwrap();

    let output = run_in(&["-r", "empty"], &[], Some(directory.path()));

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
