# Coverage Prepare

[![CI](https://github.com/samuelcolvin/coverage-prepare/actions/workflows/ci.yml/badge.svg?event=push)](https://github.com/samuelcolvin/coverage-prepare/actions/workflows/ci.yml?query=branch%3Amain)
[![Crates.io](https://img.shields.io/crates/v/coverage-prepare?color=green)](https://crates.io/crates/coverage-prepare)

Convert coverage data to HTML reports, LCOV files or terminal tables.

`coverage-prepare --help`:

```
Convert "profraw" coverage data to:
* HTML reports
* terminal table reports
* LCOV files (for upload to codecov etc.)

See https://github.com/samuelcolvin/coverage-prepare/ for more information.

USAGE:
    coverage-prepare [OPTIONS] <OUTPUT_FORMAT> [BINARIES]...

ARGS:
    <OUTPUT_FORMAT>
            output format
            
            [possible values: html, report, lcov]

    <BINARIES>...
            binary files to build coverage from

OPTIONS:
    -h, --help
            Print help information

        --ignore-filename-regex <IGNORE_FILENAME_REGEX>
            maps to the `--ignore-filename-regex` argument to `llvm-cov`, `\.cargo/registry` &
            `library/std` are always ignored, repeat to ignore multiple filenames

        --no-delete
            whether to not delete the processed `.profraw` files and the generated `.profdata` file
            after generating the coverage reports, by default these files are deleted

    -o, --output-path <OUTPUT_PATH>
            Output path, defaults to `rust_coverage.lcov` for lcov output, and `htmlcov/rust` for
            html output

    -V, --version
            Print version information
```
