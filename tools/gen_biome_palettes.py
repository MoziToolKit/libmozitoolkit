#!/usr/bin/env python3
"""Regenerate `crates/mtk-material/src/biome/canonical_data.rs` from the shipped game data.

Ground truth:
  * `<assets>/data/minecraft/worldgen/biome/*.json` - authoritative per biome
    (`temperature`, `downfall`, `effects.{water,grass,foliage,dry_foliage}_color`,
    `effects.grass_color_modifier`);
  * `<assets>/assets/minecraft/textures/colormap/{grass,foliage,dry_foliage}.png` - the lookup
    tables Minecraft samples for biomes without an explicit colour.

The output must satisfy `crates/mtk-material/tests/test_biome_matches_jar.rs`; run this script when
the game version changes, then run that test.

Usage:
    python3 tools/gen_biome_palettes.py [--assets ../Minecraft_Source] [--out <file>]
"""

from __future__ import annotations

import argparse
import json
import struct
import zlib
from pathlib import Path

# Minecraft's GrassColorModifier.SWAMP returns one of two fixed colours; the table stores the
# high-noise one (`get_swamp_grass_color` picks between them per block).
SWAMP_HIGH_HEX = "#6A7039"


def read_png(path: Path) -> tuple[int, int, int, bytes]:
    """Minimal PNG reader for the 8-bit, non-interlaced colormaps Mojang ships."""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", f"{path}: not a PNG"
    pos = 8
    width = height = channels = 0
    idat = bytearray()
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, bit_depth, color_type, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert bit_depth == 8 and interlace == 0, f"{path}: unsupported PNG layout"
            channels = {2: 3, 6: 4}[color_type]
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    raw = zlib.decompress(bytes(idat))
    stride = width * channels
    out = bytearray(stride * height)
    prev = bytearray(stride)
    offset = 0
    for row in range(height):
        filter_type = raw[offset]
        offset += 1
        line = bytearray(raw[offset : offset + stride])
        offset += stride
        for i in range(stride):
            left = line[i - channels] if i >= channels else 0
            up = prev[i]
            up_left = prev[i - channels] if i >= channels else 0
            if filter_type == 1:
                line[i] = (line[i] + left) & 0xFF
            elif filter_type == 2:
                line[i] = (line[i] + up) & 0xFF
            elif filter_type == 3:
                line[i] = (line[i] + ((left + up) >> 1)) & 0xFF
            elif filter_type == 4:
                p = left + up - up_left
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - up_left)
                best = left if (pa <= pb and pa <= pc) else (up if pb <= pc else up_left)
                line[i] = (line[i] + best) & 0xFF
            elif filter_type != 0:
                raise ValueError(f"{path}: unknown filter {filter_type}")
        out[row * stride : (row + 1) * stride] = line
        prev = line
    return width, height, channels, bytes(out)


def as_f32(value: float) -> float:
    """Round a Python double to `f32`, the width Minecraft's climate settings use.

    `Biome.ClimateSettings` stores temperature/downfall as floats, so `1.0 - 0.6f` is
    0.3999999761581421 (not 0.4) and the colormap index truncates one texel lower. Doing this in a
    double would pick a different texel than the game.
    """
    return struct.unpack("f", struct.pack("f", value))[0]


def sample(colormap: tuple[int, int, int, bytes], temp: float, downfall: float) -> tuple[int, int, int]:
    """`ColorMapColorUtil.get`: x = (int)((1-temp)*255), y = (int)((1-rain)*255), index = y<<8|x."""
    width, _height, channels, pixels = colormap
    # `sample_colormap_pixel_u8` clamps both inputs; nether biomes declare temperature 2.0.
    temp = min(max(as_f32(temp), 0.0), 1.0)
    downfall = min(max(as_f32(downfall), 0.0), 1.0)
    rain = downfall * temp
    x = int((1.0 - temp) * 255.0)
    y = int((1.0 - rain) * 255.0)
    index = (y * 256 + x) * channels
    return pixels[index], pixels[index + 1], pixels[index + 2]


def dark_forest(color: tuple[int, int, int]) -> tuple[int, int, int]:
    """`GrassColorModifier.DARK_FOREST`: ((base & 0x00FEFEFE) + 0x0028340A) >> 1, on packed ARGB."""
    r, g, b = color
    packed = 0xFF000000 | (r << 16) | (g << 8) | b
    packed = ((packed & 0x00FEFEFE) + 0x0028340A) >> 1
    return (packed >> 16) & 0xFF, (packed >> 8) & 0xFF, packed & 0xFF


def to_hex(color: tuple[int, int, int]) -> str:
    return "#{:02X}{:02X}{:02X}".format(*color)


def parse_hex(value: str) -> str:
    return "#" + value.strip().lstrip("#").upper()


def rust_float(value: float) -> str:
    """Render a float the way Rust expects an `f32` literal (always with a decimal point)."""
    text = repr(float(value))
    return text if ("." in text or "e" in text) else text + ".0"


def display_name(biome_id: str) -> str:
    return " ".join(word.capitalize() for word in biome_id.split("_"))


def generate(assets: Path) -> str:
    biome_dir = assets / "data/minecraft/worldgen/biome"
    colormap_dir = assets / "assets/minecraft/textures/colormap"
    grass = read_png(colormap_dir / "grass.png")
    foliage = read_png(colormap_dir / "foliage.png")
    dry_foliage = read_png(colormap_dir / "dry_foliage.png")

    entries = []
    for path in sorted(biome_dir.glob("*.json")):
        biome_id = path.stem
        data = json.loads(path.read_text())
        effects = data.get("effects", {})
        temp = float(data["temperature"])
        downfall = float(data["downfall"])
        modifier = effects.get("grass_color_modifier")

        grass_hex = effects.get("grass_color")
        has_custom_grass = grass_hex is not None
        if grass_hex is None:
            if modifier == "dark_forest":
                grass_hex = to_hex(dark_forest(sample(grass, temp, downfall)))
                # The modifier keeps the colour fixed, so the palette marks it custom too; the
                # dedicated C-10 item will separate "explicit" from "modifier" in the data model.
                has_custom_grass = True
            elif modifier == "swamp":
                grass_hex = SWAMP_HIGH_HEX
                has_custom_grass = True
            else:
                grass_hex = to_hex(sample(grass, temp, downfall))

        foliage_hex = effects.get("foliage_color") or to_hex(sample(foliage, temp, downfall))
        dry_hex = effects.get("dry_foliage_color") or to_hex(sample(dry_foliage, temp, downfall))

        entries.append(
            dict(
                id=biome_id,
                name=display_name(biome_id),
                grass_hex=parse_hex(grass_hex),
                foliage_hex=parse_hex(foliage_hex),
                dry_foliage_hex=parse_hex(dry_hex),
                water_hex=parse_hex(effects["water_color"]),
                temperature=temp,
                humidity=downfall,
                has_custom_grass=has_custom_grass,
                has_custom_foliage=effects.get("foliage_color") is not None,
                has_custom_dry_foliage=effects.get("dry_foliage_color") is not None,
            )
        )

    lines = [
        "//! Canonical static definitions of Minecraft biomes.",
        "//!",
        "//! GENERATED by `tools/gen_biome_palettes.py` from the shipped game data",
        "//! (`data/minecraft/worldgen/biome/*.json` plus the colormap textures). Do not edit by hand:",
        "//! `crates/mtk-material/tests/test_biome_matches_jar.rs` fails when this table disagrees with",
        "//! the assets, and the generator is the only supported way to refresh it.",
        "",
        "use super::BiomePalette;",
        "",
        "pub static CANONICAL_BIOMES: &[BiomePalette] = &[",
    ]
    for entry in entries:
        lines.extend(
            [
                "    BiomePalette {",
                f'        id: "{entry["id"]}",',
                f'        name: "{entry["name"]}",',
                f'        grass_hex: "{entry["grass_hex"]}",',
                f'        foliage_hex: "{entry["foliage_hex"]}",',
                f'        dry_foliage_hex: "{entry["dry_foliage_hex"]}",',
                f'        water_hex: "{entry["water_hex"]}",',
                f'        temperature: {rust_float(entry["temperature"])},',
                f'        humidity: {rust_float(entry["humidity"])},',
                f'        has_custom_grass: {str(entry["has_custom_grass"]).lower()},',
                f'        has_custom_foliage: {str(entry["has_custom_foliage"]).lower()},',
                f'        has_custom_dry_foliage: {str(entry["has_custom_dry_foliage"]).lower()},',
                "    },",
            ]
        )
    lines.append("];")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--assets", default="../Minecraft_Source")
    parser.add_argument(
        "--out", default="crates/mtk-material/src/biome/canonical_data.rs"
    )
    args = parser.parse_args()
    output = generate(Path(args.assets))
    Path(args.out).write_text(output)
    print(f"wrote {args.out} ({output.count('BiomePalette {')} biomes)")


if __name__ == "__main__":
    main()
