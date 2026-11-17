#!/usr/bin/env python3
"""Export the R35 fixture's controlled P6 captures without altering RGB bytes.

Usage: python3 tools/export-accessibility-captures.py INPUT_DIR OUTPUT_DIR
Uses only the standard library; no image/colour-profile dependency is needed.
"""

import argparse
from pathlib import Path
import struct
import zlib


def chunk(kind: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + kind
        + payload
        + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
    )


def export(source: Path, destination: Path) -> None:
    # This fixture writes exactly this header (no comments or optional fields).
    magic, dimensions, maximum, rgb = source.read_bytes().split(b"\n", 3)
    width, height = map(int, dimensions.split())
    if magic != b"P6" or maximum != b"255" or width <= 0 or height <= 0:
        raise ValueError(f"Unsupported fixture header: {source.name}")
    stride = width * 3
    if len(rgb) != stride * height:
        raise ValueError(f"Incomplete fixture pixels: {source.name}")
    # PNG filter 0 preserves each scanline verbatim before lossless compression.
    scanlines = b"".join(
        b"\0" + rgb[start : start + stride]
        for start in range(0, len(rgb), stride)
    )
    compressed = zlib.compress(scanlines)
    if zlib.decompress(compressed) != scanlines:
        raise ValueError("PNG compression changed fixture pixels")
    destination.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", compressed)
        + chunk(b"IEND", b"")
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    sources = sorted(args.input.glob("*.ppm"))
    if not sources:
        parser.error("no fixture .ppm captures found")
    args.output.mkdir(parents=True, exist_ok=True)
    for source in sources:
        export(source, args.output / (source.stem + ".png"))
    print(f"Exported {len(sources)} byte-preserving PNG captures")


if __name__ == "__main__":
    main()
