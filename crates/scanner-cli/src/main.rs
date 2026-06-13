//! scanner-cli — Command-line interface for the scanner library.
//!
//! Usage:
//!   scanner scan <image>              — Scan a file
//!   scanner scan <dir> --recursive    — Scan all images in a directory
//!   scanner scan --format qr <image>  — Scan for specific formats
//!   scanner live                      — Real-time webcam scanning
//!   scanner live --camera 1           — Specific camera index
//!   scanner live --list               — List available cameras
//!   scanner bench <dir>               — Benchmark on a folder of images

use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use scanner_core::{BarcodeFormat, PixelFormat, ScanSession, Scanner, ScannerConfig};
use std::path::PathBuf;
use std::time::Instant;

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "scanner",
    about = "High-performance QR code and barcode scanner",
    long_about = "A real-time QR code and barcode scanner powered by a pure-Rust engine.\nSupports: QR, DataMatrix, Aztec, PDF417, Code128/39/93, EAN-13/8, UPC-A/E, ITF, Codabar",
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

    /// Real-time barcode scanning from webcam
    Live {
        /// Camera index (0 = default camera)
        #[arg(short, long, default_value = "0")]
        camera: u32,

        /// List available cameras and exit
        #[arg(short, long)]
        list: bool,

        /// Barcode formats to detect (comma-separated, default: all)
        #[arg(short, long, value_delimiter = ',')]
        format: Vec<FormatArg>,

        /// ROI size as fraction of frame (0.1–1.0, default: 0.7)
        #[arg(long, default_value = "0.7")]
        roi: f32,

        /// Cooldown between re-scanning the same code (milliseconds)
        #[arg(long, default_value = "2000")]
        cooldown: u64,

        /// Target frame width (camera will pick closest supported)
        #[arg(long, default_value = "1280")]
        width: u32,

        /// Target frame height
        #[arg(long, default_value = "720")]
        height: u32,
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

    let log_level = if cli.verbose { "debug" } else { "warn" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    match cli.command {
        Command::Scan {
            path,
            recursive,
            format,
            output,
            try_rotations,
        } => {
            run_scan(path, recursive, format, output, try_rotations);
        }

        Command::Live {
            camera,
            list,
            format,
            roi,
            cooldown,
            width,
            height,
        } => {
            run_live(camera, list, format, roi, cooldown, width, height);
        }

        Command::Bench { path, iterations } => {
            run_benchmark(&path, iterations);
        }
    }
}

// ── Scan command ──────────────────────────────────────────────────────────────

fn run_scan(
    path: PathBuf,
    recursive: bool,
    format: Vec<FormatArg>,
    output: OutputFormat,
    try_rotations: bool,
) {
    let formats: Vec<BarcodeFormat> = format.into_iter().map(Into::into).collect();
    let config = ScannerConfig {
        formats: if formats.is_empty() {
            None
        } else {
            Some(formats)
        },
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
                            println!("  {} {}", format!("[{}]", r.format).cyan(), r.text.bold(),);
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

// ── Live command ──────────────────────────────────────────────────────────────

fn run_live(
    camera_index: u32,
    list_cameras: bool,
    format: Vec<FormatArg>,
    roi: f32,
    cooldown_ms: u64,
    _width: u32,
    _height: u32,
) {
    use nokhwa::{
        pixel_format::RgbFormat,
        query,
        utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType},
        Camera,
    };
    use scanner_core::Roi;

    // ── List cameras mode ────────────────────────────────────────────────────
    if list_cameras {
        println!("{}", "Available cameras:".bold());
        match query(ApiBackend::Auto) {
            Ok(cameras) => {
                if cameras.is_empty() {
                    println!("  {}", "No cameras found.".yellow());
                } else {
                    for cam in &cameras {
                        println!(
                            "  {} {} — {}",
                            format!("[{}]", cam.index()).cyan(),
                            cam.human_name().bold(),
                            cam.description().dimmed()
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("{}: {e}", "Failed to query cameras".red());
            }
        }
        return;
    }

    // ── Setup scan session ───────────────────────────────────────────────────
    let formats: Vec<BarcodeFormat> = format.into_iter().map(Into::into).collect();
    let roi_size = roi.clamp(0.1, 1.0);

    let config = ScannerConfig {
        formats: if formats.is_empty() {
            None
        } else {
            Some(formats)
        },
        preprocessing: true,
        try_rotations: false,
        try_invert: true,
        max_results: 1,
        roi: Some(Roi::centered(roi_size)),
        dedup_cooldown_ms: cooldown_ms,
    };

    let mut session = ScanSession::new(config);

    // ── Open camera ──────────────────────────────────────────────────────────
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);

    let mut camera = match Camera::new(CameraIndex::Index(camera_index), requested) {
        Ok(cam) => cam,
        Err(e) => {
            eprintln!(
                "{}: camera index {} — {e}",
                "Failed to open camera".red(),
                camera_index
            );
            eprintln!(
                "Run {} to see available cameras.",
                "'scanner live --list'".cyan()
            );
            std::process::exit(1);
        }
    };

    if let Err(e) = camera.open_stream() {
        eprintln!("{}: {e}", "Failed to start camera stream".red());
        std::process::exit(1);
    }

    let cam_info = camera.info().clone();
    let cam_res = camera.resolution();

    println!("{}", "━".repeat(60).dimmed());
    println!("  {} {}", "📷 Camera:".bold(), cam_info.human_name().cyan());
    println!(
        "  {} {}×{}",
        "   Resolution:".dimmed(),
        cam_res.width_x,
        cam_res.height_y
    );
    println!(
        "  {} {}%",
        "   ROI:".dimmed(),
        (roi_size * 100.0).round() as u32
    );
    println!("  {} {}ms", "   Cooldown:".dimmed(), cooldown_ms);
    println!("{}", "━".repeat(60).dimmed());
    println!(
        "  {} Point the camera at a barcode. Press {} to stop.",
        "▶".green(),
        "Ctrl+C".yellow()
    );
    println!("{}", "━".repeat(60).dimmed());

    // ── Ctrl+C handler ───────────────────────────────────────────────────────
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    let mut frame_count = 0u64;
    let mut detection_count = 0u64;
    let session_start = Instant::now();

    // ── Main loop ────────────────────────────────────────────────────────────
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        let frame = match camera.frame() {
            Ok(f) => f,
            Err(e) => {
                log::warn!("Frame capture error: {e}");
                continue;
            }
        };

        // Decode the frame to raw RGB bytes
        let decoded = match frame.decode_image::<RgbFormat>() {
            Ok(img) => img,
            Err(e) => {
                log::warn!("Frame decode error: {e}");
                continue;
            }
        };

        frame_count += 1;

        let frame_width = decoded.width();
        let frame_height = decoded.height();
        let raw_rgb: Vec<u8> = decoded.into_raw();

        // Feed to the scan session
        if let Some(results) =
            session.process_frame(&raw_rgb, frame_width, frame_height, PixelFormat::Rgb)
        {
            for r in &results {
                detection_count += 1;
                println!(
                    "  {} {} {}  {}",
                    "✓".green().bold(),
                    format!("[{}]", r.format).cyan(),
                    r.text.bold(),
                    format!("({:.1}ms)", session.stats().last_scan_time_ms).dimmed()
                );
            }
        }

        // Print stats every 100 frames
        if frame_count.is_multiple_of(100) {
            let stats = session.stats();
            let elapsed = session_start.elapsed().as_secs_f64();
            let fps = frame_count as f64 / elapsed;
            eprint!(
                "\r  {} frames={} fps={:.1} scan_avg={:.1}ms detections={} skip=1/{}  ",
                "●".dimmed(),
                frame_count,
                fps,
                stats.avg_scan_time_ms,
                detection_count,
                session.skip_interval()
            );
        }
    }

    // ── Cleanup ──────────────────────────────────────────────────────────────
    let _ = camera.stop_stream();

    println!();
    println!("{}", "━".repeat(60).dimmed());
    let stats = session.stats();
    let elapsed = session_start.elapsed().as_secs_f64();
    println!(
        "  Session: {:.1}s  |  {} frames  |  avg {:.1}ms/scan  |  {} codes detected",
        elapsed, stats.frames_submitted, stats.avg_scan_time_ms, stats.codes_detected
    );
    println!("{}", "━".repeat(60).dimmed());
}

// ── Bench command ─────────────────────────────────────────────────────────────

fn run_benchmark(path: &PathBuf, iterations: u32) {
    let scanner = Scanner::with_defaults();
    let files = collect_image_files(path, true);

    if files.is_empty() {
        eprintln!("{}", "No image files found for benchmark.".yellow());
        return;
    }

    println!(
        "{}",
        format!(
            "Benchmarking {} images × {} iterations",
            files.len(),
            iterations
        )
        .bold()
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
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

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
