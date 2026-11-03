//! Result types returned by the scanner.

use serde::{Deserialize, Serialize};

/// A 2D point representing a corner of a detected barcode.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate, in pixels from the left edge.
    pub x: f32,
    /// Vertical coordinate, in pixels from the top edge.
    pub y: f32,
}

impl Point {
    /// Create a point.
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Barcode symbology.
///
/// Serialized in `snake_case` (`"qr_code"`, `"ean_13"`…): this is the spelling
/// used by the JavaScript API for both the `formats` option and results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarcodeFormat {
    // ── 2D ──
    /// QR Code (ISO/IEC 18004).
    QrCode,
    /// Micro QR. rxing has no dedicated reader: it is decoded by the QR reader
    /// and reported as [`BarcodeFormat::QrCode`].
    MicroQr,
    /// Data Matrix (ISO/IEC 16022).
    DataMatrix,
    /// Aztec (ISO/IEC 24778).
    Aztec,
    /// PDF417 (ISO/IEC 15438).
    Pdf417,

    // ── 1D ──
    /// Code 128 (ISO/IEC 15417).
    #[serde(rename = "code_128")]
    Code128,
    /// Code 39 (ISO/IEC 16388). No mandatory check digit.
    #[serde(rename = "code_39")]
    Code39,
    /// Code 93.
    #[serde(rename = "code_93")]
    Code93,
    /// EAN-13 (ISO/IEC 15420).
    #[serde(rename = "ean_13")]
    Ean13,
    /// EAN-8 (ISO/IEC 15420).
    #[serde(rename = "ean_8")]
    Ean8,
    /// UPC-A (ISO/IEC 15420).
    UpcA,
    /// UPC-E, the 6-digit zero-suppressed UPC (ISO/IEC 15420).
    UpcE,
    /// Interleaved 2 of 5 (ISO/IEC 16390).
    Itf,
    /// Codabar.
    Codabar,

    /// Any other format not explicitly listed
    Unknown,
}

impl BarcodeFormat {
    /// Every format the scanner can report (everything except `Unknown`).
    pub const SUPPORTED: [BarcodeFormat; 14] = [
        BarcodeFormat::QrCode,
        BarcodeFormat::MicroQr,
        BarcodeFormat::DataMatrix,
        BarcodeFormat::Aztec,
        BarcodeFormat::Pdf417,
        BarcodeFormat::Code128,
        BarcodeFormat::Code39,
        BarcodeFormat::Code93,
        BarcodeFormat::Ean13,
        BarcodeFormat::Ean8,
        BarcodeFormat::UpcA,
        BarcodeFormat::UpcE,
        BarcodeFormat::Itf,
        BarcodeFormat::Codabar,
    ];

    /// `true` for 1D (linear) symbologies.
    pub fn is_linear(&self) -> bool {
        matches!(
            self,
            BarcodeFormat::Code128
                | BarcodeFormat::Code39
                | BarcodeFormat::Code93
                | BarcodeFormat::Ean13
                | BarcodeFormat::Ean8
                | BarcodeFormat::UpcA
                | BarcodeFormat::UpcE
                | BarcodeFormat::Itf
                | BarcodeFormat::Codabar
        )
    }

    /// `true` for the 1D symbologies most prone to false positives: no
    /// mandatory check digit (Code 39/93, ITF, Codabar) or very short
    /// (UPC-E: 6 digits, EAN-8). Noise and textures regularly "decode" as these.
    pub fn is_weak(&self) -> bool {
        matches!(
            self,
            BarcodeFormat::Code39
                | BarcodeFormat::Code93
                | BarcodeFormat::Itf
                | BarcodeFormat::Codabar
                | BarcodeFormat::UpcE
                | BarcodeFormat::Ean8
        )
    }

    /// Agreeing reads a `ScanSession` requires before reporting this format,
    /// given the configured `linear_confirmations`.
    ///
    /// 2D codes carry Reed–Solomon error correction: one read is enough.
    /// 1D codes need `linear` reads, weak ones one more.
    pub fn required_confirmations(&self, linear: u32) -> u32 {
        let linear = linear.max(1);
        if !self.is_linear() {
            1
        } else if self.is_weak() {
            linear + 1
        } else {
            linear
        }
    }
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

impl std::str::FromStr for BarcodeFormat {
    type Err = ();

    /// Parse a `BarcodeFormat` from a string.
    ///
    /// Accepts both the serde `snake_case` form (e.g. `"qr_code"`, `"code_128"`) and
    /// the human-readable form (e.g. `"QR Code"`, `"Code 128"`).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().replace([' ', '-'], "_").as_str() {
            "qr_code" | "qrcode" | "qr" => Ok(BarcodeFormat::QrCode),
            "micro_qr" | "microqr" => Ok(BarcodeFormat::MicroQr),
            "data_matrix" | "datamatrix" => Ok(BarcodeFormat::DataMatrix),
            "aztec" => Ok(BarcodeFormat::Aztec),
            "pdf417" | "pdf_417" => Ok(BarcodeFormat::Pdf417),
            "code_128" | "code128" => Ok(BarcodeFormat::Code128),
            "code_39" | "code39" => Ok(BarcodeFormat::Code39),
            "code_93" | "code93" => Ok(BarcodeFormat::Code93),
            "ean_13" | "ean13" => Ok(BarcodeFormat::Ean13),
            "ean_8" | "ean8" => Ok(BarcodeFormat::Ean8),
            "upc_a" | "upca" => Ok(BarcodeFormat::UpcA),
            "upc_e" | "upce" => Ok(BarcodeFormat::UpcE),
            "itf" => Ok(BarcodeFormat::Itf),
            "codabar" => Ok(BarcodeFormat::Codabar),
            _ => Ok(BarcodeFormat::Unknown),
        }
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

    /// Location of the barcode in the coordinate system of the frame that was
    /// passed to the scanner (ROI offset and rotations are already undone).
    ///
    /// 2D codes usually report 3–4 finder points; 1D codes report the two
    /// end points of the scan line that decoded them.
    pub points: Vec<Point>,

    /// Unix timestamp (milliseconds) when this scan occurred.
    pub timestamp_ms: u64,
}

impl ScanResult {
    /// Create a new ScanResult from an rxing result.
    pub(crate) fn from_rxing(result: &rxing::RXingResult) -> Self {
        let points: Vec<Point> = result
            .getPoints()
            .iter()
            .map(|p| Point::new(p.x, p.y))
            .collect();

        let timestamp_ms = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let raw = result.getRawBytes();
        Self {
            text: result.getText().to_string(),
            format: BarcodeFormat::from(*result.getBarcodeFormat()),
            raw_bytes: if raw.is_empty() {
                None
            } else {
                Some(raw.to_vec())
            },
            points,
            timestamp_ms,
        }
    }

    /// Apply a coordinate transform to every point of the result.
    pub(crate) fn map_points(&mut self, f: impl Fn(Point) -> Point) {
        for p in &mut self.points {
            *p = f(*p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str() {
        assert_eq!(
            "qr_code".parse::<BarcodeFormat>(),
            Ok(BarcodeFormat::QrCode)
        );
        assert_eq!(
            "Code 128".parse::<BarcodeFormat>(),
            Ok(BarcodeFormat::Code128)
        );
        assert_eq!("EAN-13".parse::<BarcodeFormat>(), Ok(BarcodeFormat::Ean13));
    }

    #[test]
    fn test_weak_formats() {
        assert!(BarcodeFormat::Code39.is_weak());
        assert!(BarcodeFormat::Itf.is_weak());
        assert!(BarcodeFormat::UpcE.is_weak());
        assert!(!BarcodeFormat::Ean13.is_weak());
        assert!(!BarcodeFormat::QrCode.is_weak());
        assert_eq!(BarcodeFormat::QrCode.required_confirmations(2), 1);
        assert_eq!(BarcodeFormat::Ean13.required_confirmations(2), 2);
        assert_eq!(BarcodeFormat::UpcE.required_confirmations(2), 3);
        assert_eq!(BarcodeFormat::UpcE.required_confirmations(1), 2);
    }
}
