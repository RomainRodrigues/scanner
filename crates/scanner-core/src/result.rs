//! Result types returned by the scanner.

use serde::{Deserialize, Serialize};

/// A 2D point representing a corner of a detected barcode.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// The detected barcode format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarcodeFormat {
    // 2D formats
    QrCode,
    MicroQr,
    DataMatrix,
    Aztec,
    Pdf417,

    // 1D formats
    Code128,
    Code39,
    Code93,
    Ean13,
    Ean8,
    UpcA,
    UpcE,
    /// Interleaved 2-of-5
    Itf,
    Codabar,

    /// Any other format not explicitly listed
    Unknown,
}

impl std::fmt::Display for BarcodeFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BarcodeFormat::QrCode => "QR Code",
            BarcodeFormat::MicroQr => "Micro QR",
            BarcodeFormat::DataMatrix => "Data Matrix",
            BarcodeFormat::Aztec => "Aztec",
            BarcodeFormat::Pdf417 => "PDF417",
            BarcodeFormat::Code128 => "Code 128",
            BarcodeFormat::Code39 => "Code 39",
            BarcodeFormat::Code93 => "Code 93",
            BarcodeFormat::Ean13 => "EAN-13",
            BarcodeFormat::Ean8 => "EAN-8",
            BarcodeFormat::UpcA => "UPC-A",
            BarcodeFormat::UpcE => "UPC-E",
            BarcodeFormat::Itf => "ITF",
            BarcodeFormat::Codabar => "Codabar",
            BarcodeFormat::Unknown => "Unknown",
        };
        write!(f, "{s}")
    }
}

impl From<rxing::BarcodeFormat> for BarcodeFormat {
    fn from(f: rxing::BarcodeFormat) -> Self {
        match f {
            rxing::BarcodeFormat::QR_CODE => BarcodeFormat::QrCode,
            rxing::BarcodeFormat::DATA_MATRIX => BarcodeFormat::DataMatrix,
            rxing::BarcodeFormat::AZTEC => BarcodeFormat::Aztec,
            rxing::BarcodeFormat::PDF_417 => BarcodeFormat::Pdf417,
            rxing::BarcodeFormat::CODE_128 => BarcodeFormat::Code128,
            rxing::BarcodeFormat::CODE_39 => BarcodeFormat::Code39,
            rxing::BarcodeFormat::CODE_93 => BarcodeFormat::Code93,
            rxing::BarcodeFormat::EAN_13 => BarcodeFormat::Ean13,
            rxing::BarcodeFormat::EAN_8 => BarcodeFormat::Ean8,
            rxing::BarcodeFormat::UPC_A => BarcodeFormat::UpcA,
            rxing::BarcodeFormat::UPC_E => BarcodeFormat::UpcE,
            rxing::BarcodeFormat::ITF => BarcodeFormat::Itf,
            rxing::BarcodeFormat::CODABAR => BarcodeFormat::Codabar,
            _ => BarcodeFormat::Unknown,
        }
    }
}

/// A successful scan result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// The decoded text content of the barcode.
    pub text: String,

    /// The barcode format that was detected.
    pub format: BarcodeFormat,

    /// Raw bytes of the decoded barcode (useful for binary QR codes).
    pub raw_bytes: Option<Vec<u8>>,

    /// The four corner points of the detected barcode in image coordinates.
    /// May have fewer points for 1D barcodes.
    pub points: Vec<Point>,

    /// Confidence score in range [0.0, 1.0].
    pub confidence: f32,

    /// Rotation angle of the detected barcode in degrees.
    pub orientation: f32,

    /// Unix timestamp (milliseconds) when this scan occurred.
    pub timestamp_ms: u64,
}

impl ScanResult {
    /// Create a new ScanResult from an rxing result.
    pub(crate) fn from_rxing(result: &rxing::RXingResult) -> Self {
        let points: Vec<Point> = result
            .getRXingResultPoints()
            .iter()
            .map(|p| Point::new(p.x, p.y))
            .collect();

        let timestamp_ms = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            text: result.getText().to_string(),
            format: BarcodeFormat::from(*result.getBarcodeFormat()),
            raw_bytes: {
                let b = result.getRawBytes().to_vec();
                if b.is_empty() {
                    None
                } else {
                    Some(b)
                }
            },
            points,
            confidence: 1.0, // rxing doesn't expose confidence directly
            orientation: 0.0,
            timestamp_ms,
        }
    }
}
