//! rxing backend — wraps the rxing crate to decode barcodes from grayscale images.
//!
//! rxing is a pure-Rust port of the ZXing (Zebra Crossing) Java library.
//! It supports QR Code, Data Matrix, Aztec, PDF417, Code 128/39/93, EAN-13/8,
//! UPC-A/E, ITF, Codabar, and more.

use crate::config::{format_accepted, ScannerConfig};
use crate::error::ScanError;
use crate::result::{BarcodeFormat, ScanResult};
use image::GrayImage;
use rxing::common::HybridBinarizer;
use rxing::multi::{GenericMultipleBarcodeReader, MultipleBarcodeReader};
use rxing::{
    BinaryBitmap, DecodeHints, Exceptions, Luma8Source, LuminanceSource, MultiFormatReader,
    PlanarYUVLuminanceSource, RXingResult, Reader,
};

/// How hard the decoder should look for a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    /// rxing default: a handful of scan lines for 1D codes, no rotation.
    /// This is the per-frame fast path.
    Fast,
    /// rxing `TryHarder`: many more scan lines and a 90° retry for 1D codes.
    Hard,
}

/// Wraps rxing's readers with our configuration.
pub struct RxingDecoder {
    hints_fast: DecodeHints,
    hints_hard: DecodeHints,
    /// 1D formats only, every row (see [`RxingDecoder::decode_rows`]).
    /// `None` when no 1D format is requested.
    hints_rows: Option<DecodeHints>,
    /// Formats the caller explicitly requested. When `Some`, any decoded result
    /// whose format is not in this list is discarded (post-decode filter).
    ///
    /// This is necessary because rxing's `PossibleFormats` hint is not always
    /// strictly respected.
    allowed_formats: Option<Vec<BarcodeFormat>>,
    max_results: usize,
}

impl RxingDecoder {
    /// Create a new decoder from a `ScannerConfig`.
    pub fn new(config: &ScannerConfig) -> Self {
        let mut hints_fast = build_hints(config);
        hints_fast.TryHarder = Some(false);
        let mut hints_hard = build_hints(config);
        hints_hard.TryHarder = Some(true);
        let hints_rows = linear_hints(&hints_hard);
        Self {
            hints_fast,
            hints_hard,
            hints_rows,
            allowed_formats: config.formats.clone(),
            max_results: config.max_results.max(1),
        }
    }

    /// Decode with the fast effort level.
    pub fn decode(&self, img: &GrayImage) -> Result<Vec<ScanResult>, ScanError> {
        self.decode_with(img, Effort::Fast)
    }

    /// Attempt to decode one or more barcodes from a grayscale image.
    ///
    /// Returns `Ok(vec)` with at least one result, or `Err(ScanError::NotFound)`.
    /// Result points are in the coordinate system of `img`.
    pub fn decode_with(
        &self,
        img: &GrayImage,
        effort: Effort,
    ) -> Result<Vec<ScanResult>, ScanError> {
        let (width, height) = img.dimensions();
        self.decode_raw(img.as_raw(), width, height, effort)
    }

    /// Attempt to decode an inverted image (white bars on black background).
    pub fn decode_inverted(
        &self,
        img: &GrayImage,
        effort: Effort,
    ) -> Result<Vec<ScanResult>, ScanError> {
        let (width, height) = img.dimensions();
        let inverted: Vec<u8> = img.as_raw().iter().map(|&p| 255 - p).collect();
        self.decode_raw(&inverted, width, height, effort)
    }

    /// Decode 1D codes only, looking at **every row** of `img` and nothing else.
    ///
    /// Meant for images that are already binarised row by row (the `Linear`
    /// strategy). Compared with [`Effort::Hard`] it skips:
    /// - the 2D detectors (QR, Data Matrix…), useless on such an image;
    /// - rxing's 90° retry of the whole image: a row-wise threshold destroys
    ///   vertical codes anyway. The retry only happens when the luminance
    ///   source supports rotation, so this uses `PlanarYUVLuminanceSource`
    ///   (no rotation support) instead of `Luma8Source`.
    ///
    /// Measured on a 1024×432 camera ROI (x86, native): 45 → 4 ms on a
    /// textured empty frame.
    pub fn decode_rows(&self, img: &GrayImage) -> Result<Vec<ScanResult>, ScanError> {
        let Some(hints) = &self.hints_rows else {
            return Err(ScanError::NotFound);
        };
        let (width, height) = (img.width() as usize, img.height() as usize);
        let source = PlanarYUVLuminanceSource::new_with_all(
            img.as_raw().clone(),
            width,
            height,
            0,
            0,
            width,
            height,
            false,
            false,
        )
        .map_err(map_error)?;
        self.decode_source(source, hints)
    }

    /// `luma` is borrowed, not copied: rxing ≥ 0.9.3 can read a slice
    /// (`Luma8Source::new_with_slice`), which saves one copy of the image per
    /// strategy.
    fn decode_raw(
        &self,
        luma: &[u8],
        width: u32,
        height: u32,
        effort: Effort,
    ) -> Result<Vec<ScanResult>, ScanError> {
        let hints = match effort {
            Effort::Fast => &self.hints_fast,
            Effort::Hard => &self.hints_hard,
        };
        let source = Luma8Source::new_with_slice(luma, width, height).map_err(map_error)?;
        self.decode_source(source, hints)
    }

    fn decode_source<S: LuminanceSource>(
        &self,
        source: S,
        hints: &DecodeHints,
    ) -> Result<Vec<ScanResult>, ScanError> {
        let mut bitmap = BinaryBitmap::new(HybridBinarizer::new(source));

        let raw: Vec<RXingResult> = if self.max_results > 1 {
            let mut reader = GenericMultipleBarcodeReader::new(MultiFormatReader::default());
            reader
                .decode_multiple_with_hints(&mut bitmap, hints)
                .map_err(map_error)?
        } else {
            let mut reader = MultiFormatReader::default();
            vec![reader
                .decode_with_hints(&mut bitmap, hints)
                .map_err(map_error)?]
        };

        let results: Vec<ScanResult> = raw
            .iter()
            .map(ScanResult::from_rxing)
            // Post-decode format filter: rxing's PossibleFormats hint is not
            // strictly enforced, so this is the authoritative check.
            .filter(|r| format_accepted(self.allowed_formats.as_deref(), r.format))
            .take(self.max_results)
            .collect();

        if results.is_empty() {
            Err(ScanError::NotFound)
        } else {
            Ok(results)
        }
    }
}

/// Checksum / format failures just mean "nothing readable here" for a scanner.
fn map_error(e: Exceptions) -> ScanError {
    match e {
        Exceptions::NotFoundException(_)
        | Exceptions::ChecksumException(_)
        | Exceptions::FormatException(_) => ScanError::NotFound,
        other => ScanError::DecodingError(format!("{other:?}")),
    }
}

/// Build the rxing DecodeHints struct from our scanner configuration.
fn build_hints(config: &ScannerConfig) -> DecodeHints {
    let mut hints = DecodeHints::default();

    // Always pass an explicit list: without it rxing also runs readers for
    // symbologies we do not expose (RSS, Telepen, MaxiCode…), which only add
    // cost and false positives.
    let requested = config
        .formats
        .as_deref()
        .unwrap_or(&BarcodeFormat::SUPPORTED);
    let rxing_formats: std::collections::HashSet<rxing::BarcodeFormat> =
        requested.iter().filter_map(format_to_rxing).collect();
    hints.PossibleFormats = Some(if rxing_formats.is_empty() {
        BarcodeFormat::SUPPORTED
            .iter()
            .filter_map(format_to_rxing)
            .collect()
    } else {
        rxing_formats
    });

    hints
}

/// `hints` restricted to the 1D formats it contains, or `None` if there are none.
fn linear_hints(hints: &DecodeHints) -> Option<DecodeHints> {
    let linear: std::collections::HashSet<rxing::BarcodeFormat> = BarcodeFormat::SUPPORTED
        .iter()
        .filter(|f| f.is_linear())
        .filter_map(format_to_rxing)
        .filter(|f| {
            hints
                .PossibleFormats
                .as_ref()
                .is_some_and(|p| p.contains(f))
        })
        .collect();
    (!linear.is_empty()).then(|| {
        let mut h = hints.clone();
        h.PossibleFormats = Some(linear);
        h
    })
}

/// Convert our `BarcodeFormat` to an rxing `BarcodeFormat`.
///
/// Returns `None` for formats that have no direct rxing equivalent
/// (they will be excluded from the `PossibleFormats` hint).
fn format_to_rxing(f: &BarcodeFormat) -> Option<rxing::BarcodeFormat> {
    match f {
        // MicroQR is not a distinct rxing format; it falls under QR_CODE.
        BarcodeFormat::QrCode | BarcodeFormat::MicroQr => Some(rxing::BarcodeFormat::QR_CODE),
        BarcodeFormat::DataMatrix => Some(rxing::BarcodeFormat::DATA_MATRIX),
        BarcodeFormat::Aztec => Some(rxing::BarcodeFormat::AZTEC),
        BarcodeFormat::Pdf417 => Some(rxing::BarcodeFormat::PDF_417),
        BarcodeFormat::Code128 => Some(rxing::BarcodeFormat::CODE_128),
        BarcodeFormat::Code39 => Some(rxing::BarcodeFormat::CODE_39),
        BarcodeFormat::Code93 => Some(rxing::BarcodeFormat::CODE_93),
        BarcodeFormat::Ean13 => Some(rxing::BarcodeFormat::EAN_13),
        BarcodeFormat::Ean8 => Some(rxing::BarcodeFormat::EAN_8),
        BarcodeFormat::UpcA => Some(rxing::BarcodeFormat::UPC_A),
        BarcodeFormat::UpcE => Some(rxing::BarcodeFormat::UPC_E),
        BarcodeFormat::Itf => Some(rxing::BarcodeFormat::ITF),
        BarcodeFormat::Codabar => Some(rxing::BarcodeFormat::CODABAR),
        // Unknown has no rxing equivalent — exclude from hints.
        BarcodeFormat::Unknown => None,
    }
}
