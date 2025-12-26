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

download_texture() {
    local name="$1"
    local dest="$2"
    local temp
    temp="$(mktemp)"
    echo "Downloading ${name} -> ${dest}"
    mkdir -p "$(dirname "$dest")"

    if curl -fL "${BASE_URL}/${name}.jpg" -o "$temp"; then
        if "${CONVERT_CMD[@]}" "$temp" "$dest"; then
            rm -f "$temp"
            return 0
        fi
    fi

    if curl -fL "${BASE_URL}/${name}.png" -o "$temp"; then
        mv "$temp" "$dest"
        return 0
    fi

    echo "⚠️  Skipping ${name} (not found)."
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
download_jpg_as_png "2k_stars.jpg" "assets/textures/background/stars.png"
download_jpg_as_png "2k_stars_milky_way.jpg" "assets/textures/background/stars_milky_way.png"
download_texture "2k_phobos" "$OUT_ROOT/phobos/albedo.png"
download_texture "2k_deimos" "$OUT_ROOT/deimos/albedo.png"
download_texture "2k_io" "$OUT_ROOT/io/albedo.png"
download_texture "2k_europa" "$OUT_ROOT/europa/albedo.png"
download_texture "2k_ganymede" "$OUT_ROOT/ganymede/albedo.png"
download_texture "2k_callisto" "$OUT_ROOT/callisto/albedo.png"
download_texture "2k_mimas" "$OUT_ROOT/mimas/albedo.png"
download_texture "2k_enceladus" "$OUT_ROOT/enceladus/albedo.png"
download_texture "2k_tethys" "$OUT_ROOT/tethys/albedo.png"
download_texture "2k_dione" "$OUT_ROOT/dione/albedo.png"
download_texture "2k_rhea" "$OUT_ROOT/rhea/albedo.png"
download_texture "2k_titan" "$OUT_ROOT/titan/albedo.png"
download_texture "2k_hyperion" "$OUT_ROOT/hyperion/albedo.png"
download_texture "2k_iapetus" "$OUT_ROOT/iapetus/albedo.png"
download_texture "2k_miranda" "$OUT_ROOT/miranda/albedo.png"
download_texture "2k_ariel" "$OUT_ROOT/ariel/albedo.png"
download_texture "2k_umbriel" "$OUT_ROOT/umbriel/albedo.png"
download_texture "2k_titania" "$OUT_ROOT/titania/albedo.png"
download_texture "2k_oberon" "$OUT_ROOT/oberon/albedo.png"
download_texture "2k_triton" "$OUT_ROOT/triton/albedo.png"
download_texture "2k_proteus" "$OUT_ROOT/proteus/albedo.png"
download_texture "2k_nereid" "$OUT_ROOT/nereid/albedo.png"
download_texture "2k_larissa" "$OUT_ROOT/larissa/albedo.png"

echo ""
echo "✅ Solar System Scope textures downloaded and converted to PNG."
echo "Textures for planets (and Earth clouds/emissive map) are stored under ${OUT_ROOT}."
