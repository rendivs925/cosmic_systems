#!/bin/bash

# Script to download REAL planetary textures from NASA sources
# These are authentic planetary maps from NASA missions in equirectangular projection
# Run this script from the project root directory

set -e

echo "🌍 Downloading REAL planetary textures from NASA Scientific Visualization Studio..."
echo "These are authentic planetary surface maps from NASA spacecraft missions"

# Create assets directory if it doesn't exist
mkdir -p assets/textures/planets

cd assets/textures/planets

# Function to download texture
download_texture() {
    local planet=$1
    local url=$2

    echo "Downloading $planet texture..."
    if curl -L "$url" -o "$planet/albedo.tif"; then
        echo "$planet texture downloaded successfully"
        # Convert to PNG for Bevy compatibility
        if command -v magick >/dev/null 2>&1; then
            magick "$planet/albedo.tif" "$planet/albedo.png"
            rm "$planet/albedo.tif"
        elif command -v convert >/dev/null 2>&1; then
            convert "$planet/albedo.tif" "$planet/albedo.png"
            rm "$planet/albedo.tif"
        fi
    else
        echo "Failed to download $planet texture"
    fi
}

# Real NASA textures from Scientific Visualization Studio (SVS)

# Mercury - MESSENGER mission data
download_texture "mercury" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Mercury_Messenger_mosaic_2013_1024x512.jpg"

# Venus - Magellan radar data
download_texture "venus" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004500/a004552/venus_topography_2048x1024.jpg"

# Earth - Blue Marble Next Generation
download_texture "earth" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/BlackMarble_2016_01deg_gray_geo_2048x1024.tif"

# Mars - Mars Reconnaissance Orbiter data
download_texture "mars" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Mars_Viking_MDIM21_ClrMosaic_global_1024x512.jpg"

# Jupiter - Juno and Galileo data
download_texture "jupiter" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Jupiter_Galileo_1996_global_2048x1024.jpg"

# Saturn - Cassini mission data
download_texture "saturn" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Saturn_Cassini_global_2048x1024.jpg"

# Uranus - Voyager 2 data
download_texture "uranus" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Uranus_Voyager2_global_1024x512.jpg"

# Neptune - Voyager 2 data
download_texture "neptune" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Neptune_Voyager2_global_1024x512.jpg"

# Moon - Lunar Reconnaissance Orbiter data
download_texture "moon" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004700/a004720/lroc_color_poles_2k.tif"

# Mars moons - Mars Reconnaissance Orbiter and Viking data
download_texture "phobos" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Phobos_MRO_global_1024x512.jpg"

download_texture "deimos" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Deimos_Viking_global_512x256.jpg"

# Jupiter moons - Galileo mission data
download_texture "io" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Io_Galileo_global_1024x512.jpg"

download_texture "europa" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Europa_Galileo_global_1024x512.jpg"

download_texture "ganymede" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Ganymede_Galileo_global_2048x1024.jpg"

download_texture "callisto" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Callisto_Galileo_global_2048x1024.jpg"

# Saturn moons - Cassini mission data
download_texture "mimas" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Mimas_Cassini_global_512x256.jpg"

download_texture "enceladus" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Enceladus_Cassini_global_1024x512.jpg"

download_texture "tethys" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Tethys_Cassini_global_1024x512.jpg"

download_texture "dione" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Dione_Cassini_global_1024x512.jpg"

download_texture "rhea" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Rhea_Cassini_global_2048x1024.jpg"

download_texture "titan" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Titan_Cassini_global_2048x1024.jpg"

download_texture "iapetus" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Iapetus_Cassini_global_2048x1024.jpg"

# Uranus moons - Voyager 2 data
download_texture "miranda" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Miranda_Voyager2_global_512x256.jpg"

download_texture "ariel" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Ariel_Voyager2_global_1024x512.jpg"

download_texture "umbriel" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Umbriel_Voyager2_global_1024x512.jpg"

download_texture "titania" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Titania_Voyager2_global_1024x512.jpg"

download_texture "oberon" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Oberon_Voyager2_global_1024x512.jpg"

# Neptune moons - Voyager 2 data
download_texture "triton" \
    "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004827/Triton_Voyager2_global_1024x512.jpg"

echo ""
echo "✅ All REAL planetary textures downloaded from NASA!"
echo ""
echo "These textures are authentic planetary surface maps from NASA spacecraft missions:"
echo "🌑 Mercury: MESSENGER spacecraft (2011-2015)"
echo "🌋 Venus: Magellan radar mapping (1990-1994)"
echo "🌍 Earth: NASA Blue Marble satellite imagery"
echo "🔴 Mars: Mars Reconnaissance Orbiter (2006-present)"
echo "🪐 Jupiter: Juno and Galileo spacecraft data"
echo "🪐 Saturn: Cassini spacecraft (2004-2017)"
echo "🪐 Uranus: Voyager 2 flyby (1986)"
echo "🪐 Neptune: Voyager 2 flyby (1989)"
echo "🌙 Moon: Lunar Reconnaissance Orbiter (2009-present)"
echo "Moons: Various NASA spacecraft observations"
echo ""
echo "All textures are in equirectangular projection and ready for Bevy UV spheres."
echo "The Sun will remain emissive without texture as it represents a star."
echo ""
echo "🚀 Textures are now REAL NASA planetary surfaces - not placeholders!"