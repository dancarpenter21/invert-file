# invert

`invert` writes a byte-for-byte bitwise inversion to standard output by default.

```bash
cargo install --path .
invert picture.png
invert picture.png > picture.inv.png
invert picture.png -o output.bin
invert picture.png -o # writes picture.inv.png
invert '*.png' -o    # writes one .inv file for every matched input
some-command | invert > inverted.bin
some-command | invert -o inverted.bin
invert first.bin - second.bin > combined.inv
invert first.bin second.bin -o - | another-command
invert mime picture.inv.png
invert is picture.inv.png
invert completions install bash
```

The installer writes Bash completion to
`~/.local/share/bash-completion/completions/invert`; start a new Bash session
after installing it. `invert mime` recognizes magic bytes in either ordinary or
inverted content. `invert is` prints `true`, `false`, or `unknown`; only
`true` exits successfully. In inversion mode, omitted input and `-` both mean
standard input, and `-o -` explicitly selects standard output. Inputs are
streamed and concatenated in argument order. `-o` / `--output` redirects
inversion to a file; it also accepts piped input, while the flag with no output
value uses the conventional `.inv` filename for every file input (including
wildcard matches). An explicit output filename requires exactly one input.
