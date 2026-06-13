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
use rxing::{
    BinaryBitmap, DecodeHints, Exceptions, Luma8LuminanceSource, MultiFormatReader, Reader,
};
use rxing::common::HybridBinarizer;

/// Wraps rxing's MultiFormatReader with our configuration.
pub struct RxingDecoder {
    hints: DecodeHints,
}

impl RxingDecoder {
    /// Create a new decoder from a `ScannerConfig`.
    pub fn new(config: &ScannerConfig) -> Self {
        let hints = build_hints(config);
        Self { hints }
    }

    /// Attempt to decode one or more barcodes from a grayscale image.
    ///
    /// Returns `Ok(vec)` with at least one result, or `Err(ScanError::NotFound)`.
    pub fn decode(&self, img: &GrayImage) -> Result<Vec<ScanResult>, ScanError> {
        let (width, height) = img.dimensions();
        let luma_data: Vec<u8> = img.pixels().map(|p| p[0]).collect();

        // Wrap in rxing's Luma8LuminanceSource (available in 0.9 with `decoders` feature)
        let source = Luma8LuminanceSource::new(luma_data, width, height);
        let binarizer = HybridBinarizer::new(source);
        let mut bitmap = BinaryBitmap::new(binarizer);

        let mut reader = MultiFormatReader::default();
        let mut hints = self.hints.clone();

        match reader.decode_with_hints(&mut bitmap, &mut hints) {
            Ok(result) => {
                let scan = ScanResult::from_rxing(&result);
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

    // Map our BarcodeFormat enum to rxing formats
    if let Some(formats) = &config.formats {
        let rxing_formats: std::collections::HashSet<rxing::BarcodeFormat> =
            formats.iter().filter_map(format_to_rxing).collect();

        if !rxing_formats.is_empty() {
            hints.PossibleFormats = Some(rxing_formats);
        }
    }

    // Try harder: enables multi-scale and rotation attempts within rxing itself
    hints.TryHarder = Some(true);

    hints
}

/// Convert our `BarcodeFormat` to an rxing `BarcodeFormat`.
fn format_to_rxing(f: &BarcodeFormat) -> Option<rxing::BarcodeFormat> {
    match f {
        BarcodeFormat::QrCode | BarcodeFormat::MicroQr => {
            Some(rxing::BarcodeFormat::QR_CODE)
        }
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
        BarcodeFormat::Unknown => None,
    }
}

/// Invert a grayscale image (255 - pixel).
fn invert_grayscale(img: &GrayImage) -> GrayImage {
    let (w, h) = img.dimensions();
    let data: Vec<u8> = img.pixels().map(|p| 255u8.saturating_sub(p[0])).collect();
    GrayImage::from_raw(w, h, data).expect("invert: same dimensions")
}
