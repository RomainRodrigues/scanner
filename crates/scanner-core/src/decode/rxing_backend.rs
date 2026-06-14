//! rxing backend — wraps the rxing crate to decode barcodes from grayscale images.
//!
//! rxing is a pure-Rust port of the ZXing (Zebra Crossing) Java library.
//! It supports QR Code, Data Matrix, Aztec, PDF417, Code 128/39/93, EAN-13/8,
//! UPC-A/E, ITF, Codabar, and more.
//!
//! NOTE: rxing 0.9 changed its API:
//! - `DecodeHints` is now a plain struct (not a HashMap)
//! - `BufferedImageLuminanceSource` requires feature `image` + `decoders`
//! - `RXingResult` is available via `rxing::RXingResult`

use crate::config::ScannerConfig;
use crate::error::ScanError;
use crate::result::{BarcodeFormat, ScanResult};
use image::GrayImage;
use rxing::common::HybridBinarizer;
use rxing::{
    BinaryBitmap, DecodeHints, Exceptions, Luma8LuminanceSource, MultiFormatReader, Reader,
};

/// Wraps rxing's MultiFormatReader with our configuration.
pub struct RxingDecoder {
    hints: DecodeHints,
    /// Formats the caller explicitly requested. When `Some`, any decoded result
    /// whose format is not in this list is discarded (post-decode filter).
    ///
    /// This is necessary because rxing's `PossibleFormats` hint is not always
    /// strictly respected, especially when `TryHarder = true`.
    allowed_formats: Option<Vec<BarcodeFormat>>,
}

impl RxingDecoder {
    /// Create a new decoder from a `ScannerConfig`.
    pub fn new(config: &ScannerConfig) -> Self {
        let allowed_formats = config.formats.clone();
        let hints = build_hints(config);
        Self { hints, allowed_formats }
    }

    /// Attempt to decode one or more barcodes from a grayscale image.
    ///
    /// Returns `Ok(vec)` with at least one result, or `Err(ScanError::NotFound)`.
    ///
    /// If `allowed_formats` was configured, results with an unexpected format are
    /// rejected — even if rxing decoded something. This is the authoritative filter.
    pub fn decode(&self, img: &GrayImage) -> Result<Vec<ScanResult>, ScanError> {
        let (width, height) = img.dimensions();
        let luma_data: Vec<u8> = img.pixels().map(|p| p[0]).collect();

        // Wrap in rxing's Luma8LuminanceSource (available in 0.9 with `decoders` feature)
        let source = Luma8LuminanceSource::new(luma_data, width, height);
        let binarizer = HybridBinarizer::new(source);
        let mut bitmap = BinaryBitmap::new(binarizer);

        let mut reader = MultiFormatReader::default();
        let hints = self.hints.clone();

        match reader.decode_with_hints(&mut bitmap, &hints) {
            Ok(result) => {
                let scan = ScanResult::from_rxing(&result);

                // Post-decode format filter: rxing's PossibleFormats hint is not
                // strictly enforced (especially with TryHarder). We apply an
                // authoritative check here to guarantee callers only receive the
                // formats they requested.
                if let Some(ref allowed) = self.allowed_formats {
                    if !allowed.contains(&scan.format) {
                        return Err(ScanError::NotFound);
                    }
                }

                Ok(vec![scan])
            }
            Err(Exceptions::NotFoundException(_)) => Err(ScanError::NotFound),
            Err(e) => Err(ScanError::DecodingError(format!("{e:?}"))),
        }
    }

    /// Attempt to decode an inverted image (white bars on black background).
    pub fn decode_inverted(&self, img: &GrayImage) -> Result<Vec<ScanResult>, ScanError> {
        let inverted = invert_grayscale(img);
        self.decode(&inverted)
    }
}

/// Build the rxing DecodeHints struct from our scanner configuration.
fn build_hints(config: &ScannerConfig) -> DecodeHints {
    let mut hints = DecodeHints::default();

    // Pass requested formats as a hint to rxing (best-effort — not strictly enforced).
    // The authoritative filter is applied post-decode in `RxingDecoder::decode`.
    if let Some(formats) = &config.formats {
        let rxing_formats: std::collections::HashSet<rxing::BarcodeFormat> =
            formats.iter().filter_map(format_to_rxing).collect();

        if !rxing_formats.is_empty() {
            hints.PossibleFormats = Some(rxing_formats);
        }
    }

    // TryHarder enables multi-scale and rotation attempts inside rxing.
    // Disable it when the caller requests a strict format filter: TryHarder can
    // cause rxing to ignore PossibleFormats and try all readers anyway.
    hints.TryHarder = Some(config.formats.is_none());

    hints
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

/// Invert a grayscale image (255 - pixel).
fn invert_grayscale(img: &GrayImage) -> GrayImage {
    let (w, h) = img.dimensions();
    let data: Vec<u8> = img.pixels().map(|p| 255u8.saturating_sub(p[0])).collect();
    GrayImage::from_raw(w, h, data).expect("invert: same dimensions")
}
