use std::env::consts::EXE_SUFFIX;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fmt, fs, process};

use anyhow::Result as AnyResult;
use clap::{Parser, ValueEnum};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

const PROFDATA_FILE: &str = "rust_coverage.profdata";
const IGNORE_REGEXES: &[&str] = &["\\.cargo/registry", "library/std"];

#[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum OutputFormat {
    Html,
    Report,
    Lcov,
}

/// Convert "profraw" coverage data to:
/// * HTML reports
/// * terminal table reports
/// * LCOV files (for upload to codecov etc.)
///
/// See <https://github.com/samuelcolvin/coverage-prepare/> for more information.
#[derive(Parser, Debug)]
#[clap(author, version, verbatim_doc_comment)]
struct Cli {
    /// output format
    #[clap(arg_enum, value_parser)]
    output_format: OutputFormat,

    /// binary files to build coverage from
    #[clap(value_parser)]
    binaries: Vec<String>,

    /// Output path, defaults to `rust_coverage.lcov` for lcov output, and `htmlcov/rust` for html output
    #[clap(short, long, value_parser)]
    output_path: Option<String>,

    /// maps to the `--ignore-filename-regex` argument to `llvm-cov`, `\.cargo/registry` & `library/std`
    /// are always ignored, repeat to ignore multiple filenames
    #[clap(long, value_parser)]
    ignore_filename_regex: Vec<String>,

    /// whether to not delete the processed `.profraw` files and the generated `.profdata` file
    /// after generating the coverage reports, by default these files are deleted
    #[clap(long, value_parser)]
    no_delete: bool,
}

macro_rules! cprint {
    ($out:ident, $color:ident, $t:expr) => {{
        $out.set_color(ColorSpec::new().set_fg(Some(Color::$color))).unwrap();
        writeln!($out, $t).unwrap();
        $out.reset().unwrap();
    }};
    ($out:ident, $color:ident, $t:expr, $( $msg_args:expr ),+ ) => {{
        $out.set_color(ColorSpec::new().set_fg(Some(Color::$color))).unwrap();
        writeln!($out, $t, $( $msg_args ),+).unwrap();
        $out.reset().unwrap();
    }};
}

fn main() {
    let mut out = StandardStream::stderr(ColorChoice::Auto);
    let cli = Cli::parse();
    if cli.binaries.is_empty() {
        cprint!(out, Red, "No binary files specified");
        process::exit(1);
    }

    match run(&mut out, cli) {
        Ok(()) => (),
        Err(err) => {
            cprint!(out, Red, "{}", err);
            process::exit(1);
        }
    }
}

fn run(out: &mut StandardStream, cli: Cli) -> AnyResult<()> {
    let profraw_files = merge_raw(out)?;
    let no_delete = cli.no_delete;
    cov(out, cli)?;
    maybe_delete(out, no_delete, profraw_files)
}

fn merge_raw(out: &mut StandardStream) -> AnyResult<Vec<String>> {
    let mut profraw_files = vec![];

    for dir_entry in fs::read_dir("./")? {
        let path = dir_entry?.path();
        if path.is_file() && path.extension() == Some(OsStr::new("profraw")) {
            profraw_files.push(path.to_string_lossy().to_string());
        }
    }
    if profraw_files.is_empty() {
        return Err(StringError::new("No .profraw files found in CWD").into());
    }

    let mut args = vec!["merge", "-sparse"];
    args.extend(profraw_files.iter().map(|f| f.as_str()));
    args.extend(["-o", PROFDATA_FILE]);

    let count = profraw_files.len();
    if count == 1 {
        cprint!(
            out,
            Green,
            "Converting {} file to {}",
            profraw_files.first().unwrap(),
            PROFDATA_FILE
        );
    } else {
        cprint!(out, Green, "Merging {} .profraw files into {}", count, PROFDATA_FILE);
    }
    execute("profdata", &args, false)?;
    Ok(profraw_files)
}

fn cov(out: &mut StandardStream, cli: Cli) -> AnyResult<()> {
    let command = match cli.output_format {
        OutputFormat::Html => "show",
        OutputFormat::Report => "report",
        OutputFormat::Lcov => "export",
    };
    let profile = format!("-instr-profile={}", PROFDATA_FILE);
    let mut args = vec![command, "-Xdemangler=rustfilt", &profile];
    let mut ignore_regexes = IGNORE_REGEXES
        .iter()
        .map(|r| format!("--ignore-filename-regex={}", r))
        .collect::<Vec<String>>();
    cli.ignore_filename_regex
        .iter()
        .for_each(|r| ignore_regexes.push(format!("--ignore-filename-regex={}", r)));

    args.extend(ignore_regexes.iter().map(|f| f.as_str()));
    args.extend(cli.binaries.iter().map(|f| f.as_str()));
    let mut capture = false;
    let mut output_path = ".".to_string();

    match cli.output_format {
        OutputFormat::Html => {
            output_path = cli.output_path.unwrap_or_else(|| "htmlcov/rust".to_string());
            cprint!(out, Green, "Writing HTML coverage to {}", output_path);
            args.extend(["-format=html", "-o", &output_path]);
        }
        OutputFormat::Report => {
            cprint!(out, Green, "Generating coverage report");
        }
        OutputFormat::Lcov => {
            output_path = cli.output_path.unwrap_or_else(|| "rust_coverage.lcov".to_string());
            cprint!(out, Green, "Exporting coverage data to {}", output_path);
            args.push("-format=lcov");
            capture = true;
        }
    };

    let output = execute("cov", &args, capture)?;
    if let Some(output) = output {
        let mut file = File::create(output_path)?;
        file.write_all(&output)?;
    }
    Ok(())
}

fn maybe_delete(out: &mut StandardStream, no_delete: bool, profraw_files: Vec<String>) -> AnyResult<()> {
    let mut to_delete = profraw_files;
    to_delete.push(PROFDATA_FILE.to_string());
    if no_delete {
        cprint!(out, White, "--no-delete set, not deleting {}", to_delete.join(", "));
    } else {
        cprint!(out, White, "Deleting {} coverage files", to_delete.len());
        for file in to_delete {
            fs::remove_file(file)?;
        }
    }
    Ok(())
}

fn execute(tool_name: &str, args: &[&str], capture: bool) -> Result<Option<Vec<u8>>, StringError> {
    let path = path(tool_name).map_err(|e| StringError::new(format!("Failed to find tool: {}\n{}", tool_name, e)))?;

    if !path.exists() {
        return Err(StringError::new(format!(
            "Could not find tool: {}\nat: {}\nConsider `rustup component add llvm-tools-preview`",
            tool_name,
            path.to_string_lossy()
        )));
    };

    let cmd_display = format!("{} {}", tool_name, args.join(" "));

    let status = if capture {
        let output = match Command::new(path).args(args).output() {
            Err(e) => return Err(StringError::new(format!("Failed to execute: {}\n{}", cmd_display, e))),
            Ok(s) => s,
        };

        if output.status.success() {
            return Ok(Some(output.stdout));
        } else {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            output.status
        }
    } else {
        match Command::new(path).args(args).status() {
            Err(e) => return Err(StringError::new(format!("Failed to execute: {}\n{}", cmd_display, e))),
            Ok(s) => s,
        }
    };
    match status.code() {
        Some(0) => Ok(None),
        Some(status_code) => Err(StringError::new(format!(
            "Command \"{}\" exited with status code: {}",
            cmd_display, status_code
        ))),
        None => Err(StringError::new(format!(
            "Failed to execute command: \"{}\"",
            cmd_display
        ))),
    }
}

#[derive(Debug, Clone)]
struct StringError {
    message: String,
}

impl StringError {
    fn new<S: Into<String>>(message: S) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for StringError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for StringError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

fn path(tool_name: &str) -> AnyResult<PathBuf> {
    let mut path = rustlib()?;
    path.push(format!("llvm-{}{}", tool_name, EXE_SUFFIX));
    Ok(path)
}

// see https://github.com/rust-embedded/cargo-binutils/blob/36102732f7535b4730f7cd66c670ebe3959994ef/src/rustc.rs#L7-L23
pub fn sysroot() -> AnyResult<String> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(rustc).arg("--print").arg("sysroot").output()?;
    // Note: We must trim() to remove the `\n` from the end of stdout
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

pub fn rustlib() -> AnyResult<PathBuf> {
    let sysroot = sysroot()?;
    let mut pathbuf = PathBuf::from(sysroot);
    pathbuf.push("lib");
    pathbuf.push("rustlib");
    pathbuf.push(rustc_version::version_meta()?.host);
    pathbuf.push("bin");
    Ok(pathbuf)
}
