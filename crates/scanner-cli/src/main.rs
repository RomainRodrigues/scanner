//! scanner-cli — Command-line interface for the scanner library.
//!
//! Usage:
//!   scanner scan <image>              — Scan a file
//!   scanner scan <dir> --recursive    — Scan all images in a directory
//!   scanner scan --format qr <image>  — Scan for specific formats
//!   scanner bench <dir>               — Benchmark on a folder of images

use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use scanner_core::{BarcodeFormat, Scanner, ScannerConfig};
use std::path::PathBuf;
use std::time::Instant;

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "scanner",
    about = "High-performance QR code and barcode scanner",
    version,
    author
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Enable verbose debug output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Scan image files for barcodes
    Scan {
        /// Image file or directory to scan
        path: PathBuf,

        /// Scan directories recursively
        #[arg(short, long)]
        recursive: bool,

        /// Barcode formats to detect (comma-separated, default: all)
        #[arg(short, long, value_delimiter = ',')]
        format: Vec<FormatArg>,

        /// Output format
        #[arg(short, long, value_enum, default_value = "pretty")]
        output: OutputFormat,

        /// Enable rotation retries (useful for tilted images)
        #[arg(long)]
        try_rotations: bool,
    },

    /// Benchmark scanner performance on a folder of images
    Bench {
        /// Directory containing test images
        path: PathBuf,

        /// Number of iterations per image
        #[arg(short, long, default_value = "10")]
        iterations: u32,
    },
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Pretty,
    Json,
    Tsv,
}

#[derive(Clone, ValueEnum, Debug)]
enum FormatArg {
    Qr,
    DataMatrix,
    Aztec,
    Pdf417,
    Code128,
    Code39,
    Code93,
    Ean13,
    Ean8,
    UpcA,
    UpcE,
    Itf,
    Codabar,
}

impl From<FormatArg> for BarcodeFormat {
    fn from(f: FormatArg) -> Self {
        match f {
            FormatArg::Qr => BarcodeFormat::QrCode,
            FormatArg::DataMatrix => BarcodeFormat::DataMatrix,
            FormatArg::Aztec => BarcodeFormat::Aztec,
            FormatArg::Pdf417 => BarcodeFormat::Pdf417,
            FormatArg::Code128 => BarcodeFormat::Code128,
            FormatArg::Code39 => BarcodeFormat::Code39,
            FormatArg::Code93 => BarcodeFormat::Code93,
            FormatArg::Ean13 => BarcodeFormat::Ean13,
            FormatArg::Ean8 => BarcodeFormat::Ean8,
            FormatArg::UpcA => BarcodeFormat::UpcA,
            FormatArg::UpcE => BarcodeFormat::UpcE,
            FormatArg::Itf => BarcodeFormat::Itf,
            FormatArg::Codabar => BarcodeFormat::Codabar,
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
            .init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
            .init();
    }

    match cli.command {
        Command::Scan {
            path,
            recursive,
            format,
            output,
            try_rotations,
        } => {
            let formats: Vec<BarcodeFormat> = format.into_iter().map(Into::into).collect();
            let config = ScannerConfig {
                formats: if formats.is_empty() { None } else { Some(formats) },
                try_rotations,
                max_results: 10,
                ..ScannerConfig::image_file()
            };

            let scanner = Scanner::new(config);
            let files = collect_image_files(&path, recursive);

            if files.is_empty() {
                eprintln!("{}", "No image files found.".yellow());
                std::process::exit(1);
            }

            let mut any_found = false;
            let mut results_json = Vec::new();

            for file in &files {
                let path_str = file.to_string_lossy();
                match scanner.scan_file(&path_str) {
                    Ok(results) => {
                        any_found = true;
                        match output {
                            OutputFormat::Pretty => {
                                println!("{}", format!("✓ {path_str}").green());
                                for r in &results {
                                    println!(
                                        "  {} {}  {}",
                                        format!("[{}]", r.format).cyan(),
                                        r.text.bold(),
                                        format!("({:.0}ms)", r.timestamp_ms).dimmed()
                                    );
                                }
                            }
                            OutputFormat::Json => {
                                results_json.push(serde_json::json!({
                                    "file": path_str,
                                    "results": results.iter().map(|r| serde_json::json!({
                                        "text": r.text,
                                        "format": format!("{}", r.format),
                                        "confidence": r.confidence,
                                    })).collect::<Vec<_>>()
                                }));
                            }
                            OutputFormat::Tsv => {
                                for r in &results {
                                    println!("{}\t{}\t{}", path_str, r.format, r.text);
                                }
                            }
                        }
                    }
                    Err(scanner_core::ScanError::NotFound) => {
                        if matches!(output, OutputFormat::Pretty) {
                            println!("{}", format!("✗ {path_str}").dimmed());
                        }
                    }
                    Err(e) => {
                        eprintln!("{}: {e}", format!("Error: {path_str}").red());
                    }
                }
            }

            if matches!(output, OutputFormat::Json) {
                println!("{}", serde_json::to_string_pretty(&results_json).unwrap());
            }

            if !any_found {
                eprintln!("{}", "No barcodes found in any file.".yellow());
                std::process::exit(2);
            }
        }

        Command::Bench { path, iterations } => {
            run_benchmark(&path, iterations);
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn collect_image_files(path: &PathBuf, recursive: bool) -> Vec<PathBuf> {
    let extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "tif"];

    if path.is_file() {
        return vec![path.clone()];
    }

    let mut files = Vec::new();
    collect_images_in_dir(path, recursive, &extensions, &mut files);
    files.sort();
    files
}

fn collect_images_in_dir(
    dir: &PathBuf,
    recursive: bool,
    extensions: &[&str],
    files: &mut Vec<PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && recursive {
            collect_images_in_dir(&path, recursive, extensions, files);
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            if extensions.contains(&ext.as_str()) {
                files.push(path);
            }
        }
    }
}

fn run_benchmark(path: &PathBuf, iterations: u32) {
    let scanner = Scanner::with_defaults();
    let files = collect_image_files(path, true);

    if files.is_empty() {
        eprintln!("{}", "No image files found for benchmark.".yellow());
        return;
    }

    println!(
        "{}",
        format!("Benchmarking {} images × {} iterations", files.len(), iterations).bold()
    );
    println!("{}", "─".repeat(60));

    let mut total_time_ms = 0.0f64;
    let mut total_scans = 0u64;
    let mut detected = 0u64;

    for file in &files {
        let path_str = file.to_string_lossy();
        let mut file_times = Vec::with_capacity(iterations as usize);

        for _ in 0..iterations {
            let t0 = Instant::now();
            let found = scanner.scan_file(&path_str).is_ok();
            let elapsed = t0.elapsed().as_secs_f64() * 1000.0;
            file_times.push(elapsed);
            if found {
                detected += 1;
            }
            total_scans += 1;
        }

        let avg = file_times.iter().sum::<f64>() / file_times.len() as f64;
        let min = file_times.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = file_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        total_time_ms += file_times.iter().sum::<f64>();

        println!(
            "  {} avg={:.1}ms min={:.1}ms max={:.1}ms",
            format!("{}", file.file_name().unwrap_or_default().to_string_lossy()).cyan(),
            avg,
            min,
            max
        );
    }

    println!("{}", "─".repeat(60));
    println!(
        "  Total: {} scans, avg {:.1}ms/scan, {:.1}% detection rate",
        total_scans,
        total_time_ms / total_scans as f64,
        (detected as f64 / total_scans as f64) * 100.0
    );
}
