//! File inversion and magic-byte inspection primitives for the `invert` CLI.

pub mod cli;

use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

pub const CHUNK_SIZE: usize = 1024 * 1024;
pub const MIME_PROBE_SIZE: usize = 8 * 1024;

#[derive(Debug, Error)]
pub enum InvertError {
    #[error("input file does not exist: {}", .0.display())]
    MissingInput(PathBuf),
    #[error("input pattern matched no files: {}", .0.display())]
    EmptyPattern(PathBuf),
    #[error("invalid input pattern {pattern}: {source}")]
    InvalidPattern {
        pattern: String,
        #[source]
        source: glob::PatternError,
    },
    #[error("failed while expanding input pattern {pattern}: {source}")]
    Glob {
        pattern: String,
        #[source]
        source: glob::GlobError,
    },
    #[error("output path must differ from the input path")]
    SamePath,
    #[error("failed to inspect recursive input {}: {source}", path.display())]
    InspectRecursiveInput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read directory {}: {source}", path.display())]
    ReadDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("recursive output is also a selected input: {}", .0.display())]
    OutputIsInput(PathBuf),
    #[error("multiple recursive inputs would write to the same output: {}", .0.display())]
    DuplicateOutput(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InversionState {
    Inverted,
    NotInverted,
    Unknown,
}

/// Expand a leading `~` and glob patterns in a deterministic order.
pub fn expand_inputs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, InvertError> {
    let mut expanded = Vec::new();
    for input in inputs {
        let pattern_path = expand_tilde(input);
        let pattern = pattern_path.to_string_lossy().into_owned();
        let has_magic = pattern.contains('*') || pattern.contains('?') || pattern.contains('[');

        if !has_magic {
            expanded.push(pattern_path);
            continue;
        }

        let mut matches = Vec::new();
        for entry in glob::glob(&pattern).map_err(|source| InvertError::InvalidPattern {
            pattern: pattern.clone(),
            source,
        })? {
            matches.push(entry.map_err(|source| InvertError::Glob {
                pattern: pattern.clone(),
                source,
            })?);
        }
        matches.sort();
        if matches.is_empty() {
            return Err(InvertError::EmptyPattern(pattern_path));
        }
        expanded.extend(matches);
    }
    Ok(expanded)
}

/// Expand globs and recursively collect regular files in deterministic order.
///
/// Symbolic links and special files are skipped. Overlapping input roots are
/// deduplicated by canonical path while preserving the first occurrence.
pub fn expand_inputs_recursively(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, InvertError> {
    let roots = expand_inputs(inputs)?;
    let mut files = Vec::new();
    let mut seen = HashSet::new();

    for root in roots {
        collect_regular_files(&root, &mut files, &mut seen)?;
    }
    Ok(files)
}

/// Validate that conventional sibling outputs do not overlap selected inputs
/// or one another.
pub fn validate_conventional_outputs(inputs: &[PathBuf]) -> Result<(), InvertError> {
    let mut selected = HashSet::new();
    for input in inputs {
        let canonical =
            input
                .canonicalize()
                .map_err(|source| InvertError::InspectRecursiveInput {
                    path: input.clone(),
                    source,
                })?;
        selected.insert(canonical);
    }

    let mut outputs = HashSet::new();
    for input in inputs {
        let output = output_path(input);
        let canonical = canonical_destination(&output)?;
        if selected.contains(&canonical) {
            return Err(InvertError::OutputIsInput(output));
        }
        if !outputs.insert(canonical) {
            return Err(InvertError::DuplicateOutput(output));
        }
    }
    Ok(())
}

fn collect_regular_files(
    path: &Path,
    files: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) -> Result<(), InvertError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|source| InvertError::InspectRecursiveInput {
            path: path.to_path_buf(),
            source,
        })?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        let canonical =
            path.canonicalize()
                .map_err(|source| InvertError::InspectRecursiveInput {
                    path: path.to_path_buf(),
                    source,
                })?;
        if seen.insert(canonical) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }

    let entries = fs::read_dir(path).map_err(|source| InvertError::ReadDirectory {
        path: path.to_path_buf(),
        source,
    })?;
    let mut children = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|source| InvertError::ReadDirectory {
                    path: path.to_path_buf(),
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    children.sort();
    for child in children {
        collect_regular_files(&child, files, seen)?;
    }
    Ok(())
}

/// Return the conventional output path, toggling the final `.inv` suffix.
///
/// Inverting a normally named file adds `.inv`; inverting a file whose name
/// already ends in `.inv` removes that suffix because the resulting contents
/// are no longer inverted.
pub fn output_path(input: &Path) -> PathBuf {
    let name = input.file_name().unwrap_or_default().to_string_lossy();
    let output_name = name
        .strip_suffix(".inv")
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{name}.inv"));
    input.with_file_name(output_name)
}

/// Copy `input` to `destination`, XORing every byte with `0xff`.
pub fn invert_file(input: &Path, destination: Option<&Path>) -> Result<PathBuf, InvertError> {
    let input = expand_tilde(input);
    if !input.is_file() {
        return Err(InvertError::MissingInput(input));
    }

    let destination = destination
        .map(expand_tilde)
        .unwrap_or_else(|| output_path(&input));
    let parent = parent_or_current(&destination);
    fs::create_dir_all(parent)?;

    let input_canonical = input.canonicalize()?;
    let destination_canonical = canonical_destination(&destination)?;
    if destination_canonical == input_canonical {
        return Err(InvertError::SamePath);
    }

    let mut source = File::open(&input)?;
    let mut target = File::create(&destination)?;
    invert_reader_to_writer(&mut source, &mut target)?;
    Ok(destination)
}

/// Stream bytewise-inverted data from `source` into `destination`.
///
/// This is intended for inputs, such as standard input, that have no source
/// path from which a conventional output name can be derived.
pub fn invert_reader_to_file<R: Read>(
    source: &mut R,
    destination: &Path,
) -> Result<PathBuf, InvertError> {
    let destination = expand_tilde(destination);
    let parent = parent_or_current(&destination);
    fs::create_dir_all(parent)?;

    let mut target = File::create(&destination)?;
    invert_reader_to_writer(source, &mut target)?;
    Ok(destination)
}

/// Stream bytewise-inverted data from `source` into `target`.
pub fn invert_reader_to_writer<R: Read, W: Write>(
    source: &mut R,
    target: &mut W,
) -> io::Result<()> {
    let mut buffer = vec![0_u8; CHUNK_SIZE];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        invert_bytes_in_place(&mut buffer[..read]);
        target.write_all(&buffer[..read])?;
    }
    Ok(())
}

/// Detect a MIME type by inspecting file magic bytes, including after inversion.
pub fn mime_from_file(path: &Path) -> Result<Option<String>, InvertError> {
    let probe = read_probe(&expand_tilde(path))?;
    Ok(detect_mime(&probe).or_else(|| detect_inverted_mime(&probe)))
}

/// Determine whether magic bytes identify the file as inverted.
pub fn inversion_state(path: &Path) -> Result<InversionState, InvertError> {
    let probe = read_probe(&expand_tilde(path))?;
    let raw = detect_mime(&probe).is_some();
    let inverted = detect_inverted_mime(&probe).is_some();
    Ok(match (raw, inverted) {
        (false, true) => InversionState::Inverted,
        (true, false) => InversionState::NotInverted,
        _ => InversionState::Unknown,
    })
}

pub fn detect_mime(bytes: &[u8]) -> Option<String> {
    infer::get(bytes).map(|kind| kind.mime_type().to_owned())
}

fn detect_inverted_mime(bytes: &[u8]) -> Option<String> {
    let mut inverted = bytes.to_vec();
    invert_bytes_in_place(&mut inverted);
    detect_mime(&inverted)
}

fn read_probe(path: &Path) -> Result<Vec<u8>, InvertError> {
    if !path.is_file() {
        return Err(InvertError::MissingInput(path.to_path_buf()));
    }
    let mut file = File::open(path)?;
    let mut probe = vec![0_u8; MIME_PROBE_SIZE];
    let bytes_read = file.read(&mut probe)?;
    probe.truncate(bytes_read);
    Ok(probe)
}

fn invert_bytes_in_place(bytes: &mut [u8]) {
    for byte in bytes {
        *byte ^= 0xff;
    }
}

fn expand_tilde(path: &Path) -> PathBuf {
    let Some(path) = path.to_str() else {
        return path.to_path_buf();
    };
    if path == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(remainder) = path.strip_prefix("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(remainder);
    }
    PathBuf::from(path)
}

fn canonical_destination(path: &Path) -> Result<PathBuf, InvertError> {
    if let Ok(path) = path.canonicalize() {
        return Ok(path);
    }
    let parent = parent_or_current(path);
    let parent = parent.canonicalize()?;
    Ok(parent.join(path.file_name().unwrap_or_default()))
}

fn parent_or_current(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conventional_output_names_use_inv_suffix() {
        assert_eq!(
            output_path(Path::new("sample.bin")),
            PathBuf::from("sample.bin.inv")
        );
        assert_eq!(
            output_path(Path::new("LICENSE")),
            PathBuf::from("LICENSE.inv")
        );
        assert_eq!(output_path(Path::new(".env")), PathBuf::from(".env.inv"));
        assert_eq!(
            output_path(Path::new("sample.bin.inv")),
            PathBuf::from("sample.bin")
        );
        assert_eq!(
            output_path(Path::new("sample.bin.inv.inv")),
            PathBuf::from("sample.bin.inv")
        );
    }

    #[test]
    fn identifies_raw_and_inverted_png_headers() {
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        assert_eq!(detect_mime(&png).as_deref(), Some("image/png"));
        assert!(detect_inverted_mime(&png).is_none());
        let mut inverted = png;
        invert_bytes_in_place(&mut inverted);
        assert!(detect_mime(&inverted).is_none());
        assert_eq!(
            detect_inverted_mime(&inverted).as_deref(),
            Some("image/png")
        );
    }

    #[test]
    fn inverts_files_and_classifies_the_result() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("image.png");
        fs::write(&input, [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]).unwrap();

        assert_eq!(
            inversion_state(&input).unwrap(),
            InversionState::NotInverted
        );
        let output = invert_file(&input, None).unwrap();
        assert_eq!(
            mime_from_file(&output).unwrap().as_deref(),
            Some("image/png")
        );
        assert_eq!(inversion_state(&output).unwrap(), InversionState::Inverted);
        assert_eq!(fs::read(&output).unwrap()[0], 0x76);
    }

    #[test]
    fn streams_reader_data_to_a_named_file() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("nested/output.bin");
        let input = [0x00, 0x55, 0xff];

        let path = invert_reader_to_file(&mut &input[..], &output).unwrap();

        assert_eq!(path, output);
        assert_eq!(fs::read(output).unwrap(), [0xff, 0xaa, 0x00]);
    }
}
