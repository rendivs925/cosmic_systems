#!/usr/bin/env bash

# Download Solar System Scope 2k textures and convert JPGs to PNG for Bevy.
# Run from the repository root.

set -euo pipefail

BASE_URL="https://www.solarsystemscope.com/textures/download"
OUT_ROOT="assets/textures/planets"

if command -v magick >/dev/null 2>&1; then
    CONVERT_CMD=("magick")
elif command -v convert >/dev/null 2>&1; then
    CONVERT_CMD=("convert")
else
    echo "ImageMagick (magick or convert) is required to run this script."
    exit 1
fi

mkdir -p "$OUT_ROOT"

download_png() {
    local path="$1"
    local dest="$2"
    echo "Downloading ${path} -> ${dest}"
    mkdir -p "$(dirname "$dest")"
    curl -L "${BASE_URL}/${path}" -o "$dest"
}

download_jpg_as_png() {
    local path="$1"
    local dest="$2"
    local temp="$(mktemp)"
    echo "Downloading ${path} -> ${dest}"
    mkdir -p "$(dirname "$dest")"
    curl -L "${BASE_URL}/${path}" -o "$temp"
    "${CONVERT_CMD[@]}" "$temp" "$dest"
    rm -f "$temp"
}

echo "🌌 Downloading Solar System Scope 2k textures..."

download_jpg_as_png "2k_sun.jpg" "$OUT_ROOT/sun/albedo.png"
download_jpg_as_png "2k_mercury.jpg" "$OUT_ROOT/mercury/albedo.png"
download_jpg_as_png "2k_venus_surface.jpg" "$OUT_ROOT/venus/albedo.png"
download_jpg_as_png "2k_mars.jpg" "$OUT_ROOT/mars/albedo.png"
download_jpg_as_png "2k_jupiter.jpg" "$OUT_ROOT/jupiter/albedo.png"
download_jpg_as_png "2k_saturn.jpg" "$OUT_ROOT/saturn/albedo.png"
download_jpg_as_png "2k_uranus.jpg" "$OUT_ROOT/uranus/albedo.png"
download_jpg_as_png "2k_neptune.jpg" "$OUT_ROOT/neptune/albedo.png"
download_jpg_as_png "2k_moon.jpg" "$OUT_ROOT/moon/albedo.png"
download_jpg_as_png "2k_earth_daymap.jpg" "$OUT_ROOT/earth/albedo.png"
download_jpg_as_png "2k_earth_clouds.jpg" "$OUT_ROOT/earth/clouds.png"
download_jpg_as_png "2k_earth_nightmap.jpg" "$OUT_ROOT/earth/emissive.png"
download_jpg_as_png "2k_venus_atmosphere.jpg" "$OUT_ROOT/venus/clouds.png"
download_png "2k_saturn_ring_alpha.png" "$OUT_ROOT/saturn/rings.png"

echo ""
echo "✅ Solar System Scope textures downloaded and converted to PNG."
echo "Textures for planets (and Earth clouds/emissive map) are stored under ${OUT_ROOT}."
