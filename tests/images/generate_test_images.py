#!/usr/bin/env python3
"""
Generate test barcode images for the scanner test suite.

Requires:
    pip install qrcode pillow python-barcode

Usage:
    python3 generate_test_images.py
"""

import os
import sys

try:
    import qrcode
    from PIL import Image, ImageFilter
except ImportError:
    print("Missing dependency: pip install qrcode pillow")
    sys.exit(1)

try:
    import barcode
    from barcode.writer import ImageWriter
    HAS_BARCODE = True
except ImportError:
    print("Warning: python-barcode not installed. Skipping 1D barcode generation.")
    HAS_BARCODE = False


def make_qr(data: str, path: str, **kwargs):
    qr = qrcode.QRCode(box_size=10, border=4)
    qr.add_data(data)
    qr.make(fit=True)
    img = qr.make_image(fill_color="black", back_color="white")
    img.save(path)
    print(f"  Created: {path}")


def main():
    base = os.path.dirname(os.path.abspath(__file__))

    # ── QR codes ──────────────────────────────────────────────────────────────
    qr_dir = os.path.join(base, "qr")
    os.makedirs(qr_dir, exist_ok=True)

    # Clean HD QR
    make_qr("https://github.com/romrd/scanner", os.path.join(qr_dir, "clean_hd.png"))

    # Blurry QR (simulate camera blur)
    qr = qrcode.make("Hello, Scanner! This is a blurry QR code test.")
    qr_blurry = qr.filter(ImageFilter.GaussianBlur(radius=2))
    qr_blurry.save(os.path.join(qr_dir, "blurry_camera.jpg"), quality=60)
    print(f"  Created: {os.path.join(qr_dir, 'blurry_camera.jpg')}")

    # Dark QR (low light simulation)
    qr_dark = qr.point(lambda x: int(x * 0.4))
    qr_dark.save(os.path.join(qr_dir, "dark_low_light.jpg"), quality=80)
    print(f"  Created: {os.path.join(qr_dir, 'dark_low_light.jpg')}")

    # Rotated QR (45 degrees)
    qr_rot = qr.rotate(45, expand=True, fillcolor="white")
    qr_rot.save(os.path.join(qr_dir, "rotated_45deg.png"))
    print(f"  Created: {os.path.join(qr_dir, 'rotated_45deg.png')}")

    # Inverted QR (white on black)
    from PIL import ImageOps
    qr_inv = ImageOps.invert(qr.convert("RGB"))
    qr_inv.save(os.path.join(qr_dir, "inverted.png"))
    print(f"  Created: {os.path.join(qr_dir, 'inverted.png')}")

    # Multiple QR codes in one image
    qr1 = qrcode.make("Code A")
    qr2 = qrcode.make("Code B")
    combined = Image.new("RGB", (qr1.width + qr2.width + 20, max(qr1.height, qr2.height) + 20), "white")
    combined.paste(qr1, (10, 10))
    combined.paste(qr2, (qr1.width + 10, 10))
    combined.save(os.path.join(qr_dir, "multiple.png"))
    print(f"  Created: {os.path.join(qr_dir, 'multiple.png')}")

    # Small QR in large image
    big = Image.new("RGB", (1000, 1000), "white")
    small_qr = qrcode.make("Tiny code").resize((120, 120))
    big.paste(small_qr, (440, 440))
    big.save(os.path.join(qr_dir, "tiny_in_large.png"))
    print(f"  Created: {os.path.join(qr_dir, 'tiny_in_large.png')}")

    # ── 1D Barcodes ───────────────────────────────────────────────────────────
    if HAS_BARCODE:
        barcode_dir = os.path.join(base, "barcode_1d")
        os.makedirs(barcode_dir, exist_ok=True)

        # EAN-13
        ean = barcode.get("ean13", "5901234123457", writer=ImageWriter())
        ean.save(os.path.join(barcode_dir, "ean13_clean"))
        print(f"  Created: {os.path.join(barcode_dir, 'ean13_clean.png')}")

        # Code 128
        code128 = barcode.get("code128", "SCANNER-TEST-128", writer=ImageWriter())
        code128.save(os.path.join(barcode_dir, "code128"))
        print(f"  Created: {os.path.join(barcode_dir, 'code128.png')}")

        # UPC-A
        upca = barcode.get("upca", "012345678905", writer=ImageWriter())
        upca.save(os.path.join(barcode_dir, "upc_a"))
        print(f"  Created: {os.path.join(barcode_dir, 'upc_a.png')}")

    # ── Edge cases ────────────────────────────────────────────────────────────
    edge_dir = os.path.join(base, "edge_cases")
    os.makedirs(edge_dir, exist_ok=True)

    # No code — plain photo-like image
    no_code = Image.new("RGB", (640, 480), (180, 160, 140))
    no_code.save(os.path.join(edge_dir, "no_code.jpg"))
    print(f"  Created: {os.path.join(edge_dir, 'no_code.jpg')}")

    print("\n✓ Test images generated successfully!")


if __name__ == "__main__":
    main()
