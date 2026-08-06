# invert

`invert` XORs every byte with `0xff`. Applying it twice restores the original
data. It is designed to work both on files and as a streaming Unix filter.

## Install

```bash
cargo install --path .
invert --version
invert --help
```

## Command reference

```text
invert [OPTIONS] [INPUT]...
invert mime <FILE>
invert is <FILE>
invert completions install bash
```

| Option or command | What it does |
| --- | --- |
| `INPUT` | A file to invert. Supply more than one to process them in order; use `-` for standard input. |
| `-o`, `--output [OUTPUT]` | Select an output destination. Its three forms are `-o FILE`, `-o -`, and bare `-o`. |
| `-v`, `--verbose` | Write completed input-to-output mappings to standard error. |
| `-h`, `--help` | Print help for the current command. |
| `-V`, `--version` | Print the installed version. |
| `mime FILE` | Print a MIME type recognized from ordinary or inverted magic bytes. |
| `is FILE` | Report whether a recognized signature is inverted; its exit status is suitable for scripts. |
| `completions install bash` | Generate and install Bash completion. |

Subcommands take precedence over an input with the same spelling. For example,
`invert mime image.png` runs the `mime` command; prefix a path such as `./mime`
when you mean a file with that name.

## Basic file inversion

By default, a file input is inverted to standard output. Redirect stdout when
you want to keep the result.

```bash
invert picture.png > picture.png.inv
invert archive.bin > archive.bin.inv

# Invert again to restore the original bytes.
invert picture.png.inv > picture.png
```

Use an explicit destination filename with `-o` / `--output`:

```bash
invert picture.png -o picture.png.inv
invert picture.png --output /tmp/inverted-picture.bin
```

Parent directories for a named output are created when needed. `~` is expanded
in input and output paths, including when it is quoted.

```bash
invert ~/Downloads/report.pdf -o ~/Archive/report.pdf.inv
invert '~/Downloads/report.pdf' --output '~/Archive/report.pdf.inv'
```

For safety, the output path must differ from the input path:

```bash
invert report.pdf -o report.pdf
# error: output path must differ from the input path
```

Pass `-o` with no filename to create the conventional output name beside each
input. Inverting a regular filename adds `.inv`; inverting that output again
removes `.inv` because the restored file is no longer inverted.

```bash
invert picture.png -o          # picture.png.inv
invert picture.png.inv -o      # picture.png
invert LICENSE -o              # LICENSE.inv
invert /data/photo.jpg --output # /data/photo.jpg.inv
invert .env -o                 # .env.inv
```

An explicit destination filename accepts exactly one file input:

```bash
invert input.bin -o output.bin

# This is rejected because one named destination cannot represent two inputs.
invert one.bin two.bin -o output.bin
```

The long form accepts the same three forms:

```bash
invert input.bin --output output.bin.inv  # named file
invert input.bin --output -               # standard output
invert input.bin --output                 # input.bin.inv
```

## Standard input and output

With no input arguments, `invert` reads standard input and writes binary data
to standard output. It processes a fixed-size buffer at a time, so it does not
need to hold the full stream in memory.

```bash
producer | invert > inverted.bin
cat original.bin | invert | consumer
```

`-` explicitly denotes standard input. It can be useful in scripts and can be
mixed with files; inputs are read and written in the order supplied.

```bash
producer | invert - > inverted.bin
invert first.bin - second.bin < middle.bin > combined.inv
```

Use `-o -` / `--output -` to explicitly select standard output. This is
equivalent to omitting `--output` and permits multiple inputs.

```bash
invert first.bin second.bin -o - | consumer
producer | invert --output - > inverted.bin
```

An explicit destination accepts exactly one input—either one file or `-` for
standard input:

```bash
invert source.bin -o destination.bin
printf 'data' | invert - -o destination.bin
```

Standard input may be sent directly to a named output file:

```bash
producer | invert -o inverted.bin
producer | invert --output /tmp/inverted.bin
```

Bare `-o` / `--output` is intentionally invalid for standard input because a
stream has no filename from which to derive a conventional `.inv` name.

```bash
producer | invert -o  # exits with an error
```

If a downstream command closes its pipe early, `invert` treats the resulting
broken pipe as normal completion:

```bash
invert large.bin | head -c 64 > sample.inv
```

## Multiple files and globs

Multiple inputs are concatenated to standard output after inversion.

```bash
invert header.bin body.bin footer.bin > message.inv
```

The program expands shell-style globs itself, including quoted patterns, and
sorts the matched paths deterministically. Use bare `-o` to write one
conventional output file per matched input.

```bash
invert '*.png' -o
invert 'assets/**/*.dat' --output
```

Tilde-prefixed paths are expanded as well:

```bash
invert ~/Downloads/sample.bin -o
```

An unmatched pattern is an error, which helps catch misspelled paths:

```bash
invert 'missing/*.bin' -o
# error: input pattern matched no files: missing/*.bin
```

Patterns use `*`, `?`, and `[]`; quote them when you want `invert` rather than
your shell to expand them. Quoting is especially useful when you want the
program's deterministic sorted order.

```bash
invert 'chunk-?.bin' > joined.inv
invert 'images/[ab]*.png' -o
```

## Verbose mode

`-v` / `--verbose` prints each completed input-to-output mapping to standard
error, leaving standard output safe for binary data.

```bash
invert image.png -o image.png.inv --verbose
# inverted image.png -> image.png.inv

producer | invert -o result.bin -v
# inverted <stdin> -> result.bin

invert first.bin second.bin -v > joined.inv
# inverted first.bin -> <stdout>
# inverted second.bin -> <stdout>
```

Because verbose output goes to standard error, it can be saved separately
without corrupting binary output:

```bash
invert input.bin -v > input.bin.inv 2> invert.log
```

## Inspect file signatures

`mime` examines magic bytes and prints a detected MIME type. It recognizes
both ordinary and inverted files; unknown content is reported as
`application/octet-stream`.

```bash
invert mime photo.png
# image/png

invert mime photo.png.inv
# image/png

invert mime unknown-data.bin
# application/octet-stream
```

`is` reports whether magic bytes identify a file as inverted. It prints
`true`, `false`, or `unknown`. Only `true` exits successfully, which makes it
useful in shell conditionals.

```bash
invert is photo.png.inv
# true

if invert is photo.png.inv; then
  echo 'file appears to be inverted'
fi

invert is photo.png
# false (exit status 1)
```

`is` returns status `0` only for `true`; both `false` and `unknown` return
status `1`. Read errors return status `2`.

```bash
if invert is candidate.bin; then
  invert candidate.bin -o restored.bin
else
  echo 'not a recognized inverted file' >&2
fi

invert is random.bin
# unknown (exit status 1)
```

These inspection commands take file paths; streaming input applies only to
inversion mode.

## Bash completion

Install Bash completion into the user data directory:

```bash
invert completions install bash
```

This writes `invert` completion data to
`~/.local/share/bash-completion/completions/invert` by default (or under
`$XDG_DATA_HOME` when set). Start a new Bash session afterward.

To choose an explicit data directory, set `XDG_DATA_HOME` for the command:

```bash
XDG_DATA_HOME="$PWD/.xdg-data" invert completions install bash
# installed bash completion: .../.xdg-data/bash-completion/completions/invert
```

Only Bash is currently supported. The usual help and version flags are also
available at the top level and help is available for every subcommand:

```bash
invert --version
invert --help
invert mime --help
invert is --help
invert completions --help
invert completions install --help
```

## Man page

Generate the checked-in Unix man page from the same Clap command definition
that powers `--help`:

```bash
cargo run --example generate-man
man ./man/invert.1
```
