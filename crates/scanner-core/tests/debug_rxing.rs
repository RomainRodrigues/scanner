use rxing::helpers::detect_in_file;
use rxing::BarcodeFormat;

#[test]
fn debug_decode() {
    let res = detect_in_file(
        "../../tests/images/qr/clean_hd.png",
        Some(BarcodeFormat::QR_CODE),
    );
    match res {
        Ok(r) => println!("Found: {}", r.getText()),
        Err(e) => panic!("Error: {:?}", e),
    }
}
