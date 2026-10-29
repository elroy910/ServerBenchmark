use clap::{Args, Parser, Subcommand};
use serverbench_core::{Error, Metadata, Mode, Result};
use serverbench_report::{compare_files, console, default_path, info, save_result, ProgressOutput};
use serverbench_survey::{parse_duration, run, survey, validate, RunOptions, SurveyOptions};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "serverbench",
    version,
    about = "Reproducible, offline Linux CPU capacity and consistency benchmark (experimental CPU v1)"
)]
struct Cli {
    #[arg(long, global = true, conflicts_with = "verbose")]
    quiet: bool,
    #[arg(long, global = true, conflicts_with = "quiet")]
    verbose: bool,
    /// Hide live resource updates while keeping phase messages and results.
    #[arg(long, global = true)]
    no_progress: bool,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Inspect Linux system information without a heavy benchmark.
    Info {
        #[arg(long)]
        json: bool,
    },
    /// Run CPU benchmarks; full mode is the default (~8m15s).
    Benchmark {
        #[command(flatten)]
        options: BenchmarkArgs,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compare compatible result files; incompatible differences are suppressed.
    Compare { a: PathBuf, b: PathBuf },
    /// Periodically run benchmarks and keep individual results plus a summary.
    Survey {
        #[command(flatten)]
        options: BenchmarkArgs,
        #[arg(long,default_value="24h",value_parser=duration_arg)]
        duration: u64,
        #[arg(long,default_value="2h",value_parser=duration_arg)]
        interval: u64,
        #[arg(long,value_parser=duration_arg)]
        jitter: Option<u64>,
        #[arg(short, long, default_value = "results")]
        output: PathBuf,
    },
    /// Validate known answers, datasets, reference, timer, memory and affinity.
    Validate {
        /// Trusted SHA-256 from a separately obtained release manifest.
        #[arg(long,value_parser=hash_arg)]
        expected_sha256: Option<String>,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Args, Default)]
struct BenchmarkArgs {
    #[arg(long,conflicts_with_all=["full","extended"])]
    quick: bool,
    #[arg(long,conflicts_with_all=["quick","extended"])]
    full: bool,
    #[arg(long,conflicts_with_all=["quick","full"])]
    extended: bool,
    #[arg(long,value_parser=duration_arg)]
    sustained: Option<u64>,
    #[arg(long)]
    strict: bool,
    #[arg(long)]
    label: Option<String>,
    #[arg(long)]
    provider: Option<String>,
    #[arg(long,value_parser=cost_arg,requires="currency")]
    monthly_cost: Option<f64>,
    #[arg(long,value_parser=currency_arg)]
    currency: Option<String>,
    /// Anonymous collection is always the default; no hostname is collected.
    #[arg(long)]
    anonymous: bool,
}
fn duration_arg(s: &str) -> std::result::Result<u64, String> {
    parse_duration(s).map_err(|e| e.to_string())
}
fn cost_arg(s: &str) -> std::result::Result<f64, String> {
    s.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.)
        .ok_or_else(|| "cost must be a positive finite number".into())
}
fn currency_arg(s: &str) -> std::result::Result<String, String> {
    if s.len() == 3 && s.bytes().all(|b| b.is_ascii_alphabetic()) {
        Ok(s.to_ascii_uppercase())
    } else {
        Err("currency must contain three ASCII letters, e.g. EUR".into())
    }
}
fn hash_arg(s: &str) -> std::result::Result<String, String> {
    if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(s.to_ascii_lowercase())
    } else {
        Err("expected 64 hexadecimal SHA-256 digits".into())
    }
}
impl BenchmarkArgs {
    fn run_options(self) -> RunOptions {
        RunOptions {
            mode: if self.quick {
                Mode::Quick
            } else if self.extended {
                Mode::Extended
            } else {
                Mode::Full
            },
            sustained: self.sustained,
            strict: self.strict,
            metadata: Metadata {
                label: self.label,
                provider: self.provider,
                monthly_cost: self.monthly_cost,
                currency: self.currency,
                anonymous: true,
            },
        }
    }
}
fn execute(cli: Cli) -> Result<()> {
    let quiet = cli.quiet;
    let verbose = cli.verbose;
    let mut reporter = ProgressOutput::new(!quiet, !cli.no_progress);
    let command = cli.command.unwrap_or(Command::Benchmark {
        options: BenchmarkArgs::default(),
        output: None,
    });
    match command {
        Command::Info { json } => {
            let system = serverbench_system::current()?;
            if !quiet {
                if json {
                    println!("{}", serde_json::to_string_pretty(&system)?);
                } else {
                    print!("{}", info(&system));
                }
            }
        }
        Command::Benchmark { options, output } => {
            let output = output.unwrap_or_else(default_path);
            if output.exists() {
                return Err(Error::new(
                    2,
                    "output file already exists; choose a new path",
                ));
            }
            let result = run(&options.run_options(), |event| reporter.handle(event))?;
            save_result(&output, &result)?;
            reporter.finish();
            if !quiet {
                print!("{}", console(&result));
                println!("\nJSON: {}", output.display());
            }
            if verbose {
                eprintln!(
                    "Binary SHA-256: {}\nReference SHA-256: {}",
                    result.binary_sha256.as_deref().unwrap_or("unknown"),
                    result.reference_sha256
                );
            }
        }
        Command::Compare { a, b } => {
            let text = compare_files(&a, &b)?;
            if !quiet {
                print!("{text}");
            }
        }
        Command::Survey {
            options,
            duration,
            interval,
            jitter,
            output,
        } => {
            let path = survey(
                &SurveyOptions {
                    run: options.run_options(),
                    duration,
                    interval,
                    jitter: jitter.unwrap_or(0),
                    output,
                },
                |event| reporter.handle(event),
            )?;
            reporter.finish();
            if !quiet {
                println!("Survey JSON: {}", path.display());
            }
        }
        Command::Validate {
            expected_sha256,
            json,
        } => {
            let report = validate(expected_sha256.as_deref())?;
            if !quiet {
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!("Validation passed (CPU v1 experimental)");
                    for check in &report.checks {
                        println!("  OK: {check}");
                    }
                    println!("Binary SHA-256: {}", report.binary_sha256);
                    for warning in &report.warnings {
                        println!("WARNING: {warning}");
                    }
                }
            }
        }
    }
    Ok(())
}
fn main() -> ExitCode {
    match execute(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("serverbench: {}", e.message);
            ExitCode::from(e.code)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_validation() {
        assert!(Cli::try_parse_from(["serverbench", "benchmark", "--quick", "--full"]).is_err());
        assert!(Cli::try_parse_from([
            "serverbench",
            "benchmark",
            "--monthly-cost",
            "NaN",
            "--currency",
            "EUR"
        ])
        .is_err());
        assert!(
            Cli::try_parse_from(["serverbench", "benchmark", "--monthly-cost", "48.85"]).is_err()
        );
        assert!(Cli::try_parse_from([
            "serverbench",
            "benchmark",
            "--sustained",
            "10m",
            "--anonymous"
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "serverbench",
            "survey",
            "--duration",
            "24h",
            "--interval",
            "2h",
            "--jitter",
            "5m"
        ])
        .is_ok());
        assert!(Cli::try_parse_from(["serverbench", "--quiet", "--verbose", "info"]).is_err());
        assert!(
            Cli::try_parse_from(["serverbench", "benchmark", "--quick", "--no-progress"]).is_ok()
        );
    }
}
