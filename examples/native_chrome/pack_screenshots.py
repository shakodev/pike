"""Losslessly package fixture P6 frames as PNG using only Python's stdlib.

Usage: python3 pack_screenshots.py DIRECTORY [--preserve-raw DIRECTORY]
The optional raw destination preserves the original PPM instead of deleting it.
No terminal data is read: these files are controlled native fixture screenshots.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import zlib


def chunk(kind, data):
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def encode(width, height, pixels):
    assert len(pixels) == width * height * 3
    rows = b"".join(
        b"\0" + pixels[row * width * 3 : (row + 1) * width * 3]
        for row in range(height)
    )
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">2I5B", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows))
        + chunk(b"IEND", b"")
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--preserve-raw", type=Path)
    args = parser.parse_args()
    manifest_path = args.directory / "screenshots.json"
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else []
    records = {entry["png"]: entry for entry in manifest}
    for source in sorted(args.directory.rglob("*.ppm")):
        data = source.read_bytes()
        header, dimensions, maximum, pixels = data.split(b"\n", 3)
        assert header == b"P6" and maximum == b"255"
        width, height = map(int, dimensions.split())
        destination = source.with_suffix(".png")
        png = encode(width, height, pixels)
        destination.write_bytes(png)
        records[str(destination.relative_to(args.directory))] = {
            "png": str(destination.relative_to(args.directory)),
            "width": width,
            "height": height,
            "rgb_sha256": hashlib.sha256(pixels).hexdigest(),
            "png_sha256": hashlib.sha256(png).hexdigest(),
            "source_ppm_sha256": hashlib.sha256(data).hexdigest(),
        }
        if args.preserve_raw:
            preserved = args.preserve_raw / source.relative_to(args.directory)
            preserved.parent.mkdir(parents=True, exist_ok=True)
            assert not preserved.exists(), "refusing to replace raw evidence"
            shutil.move(source, preserved)
    manifest_path.write_text(
        json.dumps([records[key] for key in sorted(records)], indent=2) + "\n"
    )


if __name__ == "__main__":
    main()
