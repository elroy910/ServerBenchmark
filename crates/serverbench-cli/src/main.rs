use clap::{Args, Parser, Subcommand, ValueEnum};
use serverbench_core::{Error, Metadata, Mode, Result};
use serverbench_report::{compare_files, console, default_path, info, save_result, ProgressOutput};
use serverbench_runner::single::run;
use serverbench_survey::{parse_duration, survey, validate, RunOptions, SurveyOptions};
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
        /// Run 1–100 sequential fresh processes; N=1 keeps the single JSON format.
        #[arg(long,default_value_t=1,value_parser=clap::value_parser!(u32).range(1..=100))]
        repeat: u32,
        /// Fixed pause between repetitions; requires --repeat >= 2. Default: 60s.
        #[arg(long,value_parser=duration_arg)]
        repeat_pause: Option<u64>,
        /// Finish remaining attempts after failures; the series still exits unsuccessfully.
        #[arg(long)]
        continue_on_error: bool,
    },
    /// Compare compatible result files; incompatible differences are suppressed.
    Compare {
        a: PathBuf,
        b: PathBuf,
        /// Explicitly assume independent runs within stable cohorts for conditional intervals.
        #[arg(long, value_enum)]
        uncertainty: Option<Uncertainty>,
    },
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
#[derive(Clone, Copy, ValueEnum)]
enum Uncertainty {
    IndependentRuns,
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
        repeat: 1,
        repeat_pause: None,
        continue_on_error: false,
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
        Command::Benchmark {
            options,
            output,
            repeat,
            repeat_pause,
            continue_on_error,
        } => {
            if repeat == 1 && (repeat_pause.is_some() || continue_on_error) {
                return Err(Error::new(
                    2,
                    "--repeat-pause and --continue-on-error require --repeat >= 2",
                ));
            }
            let output = output.unwrap_or_else(default_path);
            if output.exists() {
                return Err(Error::new(
                    2,
                    "output file already exists; choose a new path",
                ));
            }
            if repeat > 1 {
                use serverbench_core::series::RunSpec;
                use serverbench_runner::{
                    process::{signal_cancellation, ProcessExecutor},
                    repeat::{run_repeat, RepeatOptions},
                };
                let run_options = options.run_options();
                let spec = RunSpec {
                    mode: run_options.mode,
                    durations: run_options.durations()?,
                    strict: run_options.strict,
                    metadata: run_options.metadata,
                };
                let pause = repeat_pause.unwrap_or(60);
                let estimated = spec.estimated_seconds(repeat, pause)?;
                reporter.finish();
                if !quiet {
                    eprintln!("Repeat plan: {repeat} runs, {pause}s pauses; approximately {estimated}s plus initialization and batch tails.");
                }
                let (cancel, _signals) = signal_cancellation()?;
                let document = run_repeat(
                    &RepeatOptions {
                        executable: std::env::current_exe()?,
                        output: output.clone(),
                        spec,
                        count: repeat,
                        pause_seconds: pause,
                        continue_on_error,
                        quiet,
                        verbose,
                        no_progress: cli.no_progress,
                    },
                    &mut ProcessExecutor,
                    &cancel,
                    |message| {
                        if !quiet {
                            eprintln!("{message}");
                        }
                    },
                )?;
                if !quiet {
                    let loaded = serverbench_report::load_document(&output)?;
                    print!(
                        "{}",
                        serverbench_report::series_console(serverbench_report::require_series(
                            &loaded
                        )?)
                    );
                    println!("\nSeries JSON: {}", output.display());
                }
                if document.status != serverbench_core::documents::Validity::Valid {
                    return Err(Error::new(
                        1,
                        format!(
                            "repeat series incomplete or unsuccessful; see {}",
                            output.display()
                        ),
                    ));
                }
                return Ok(());
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
        Command::Compare { a, b, uncertainty } => {
            let text = if uncertainty.is_some() {
                serverbench_report::compare_files_with_uncertainty(&a, &b, true)?
            } else {
                compare_files(&a, &b)?
            };
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
        for args in [
            vec!["serverbench", "benchmark"],
            vec!["serverbench", "benchmark", "--repeat", "1"],
        ] {
            let c = Cli::try_parse_from(args).unwrap();
            assert!(matches!(
                c.command,
                Some(Command::Benchmark {
                    repeat: 1,
                    repeat_pause: None,
                    continue_on_error: false,
                    ..
                })
            ));
        }
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
