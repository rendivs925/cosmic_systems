#!/bin/bash

# Script to download planetary textures for Bevy solar system simulator
# Uses Solar System Scope textures which are already equirectangular and optimized
# Run this script from the project root directory

set -e

echo "Downloading planetary textures from Solar System Scope..."
echo "These are equirectangular textures optimized for 3D rendering"

# Create assets directory if it doesn't exist
mkdir -p assets/textures/planets

cd assets/textures/planets

# Function to download texture
download_texture() {
    local planet=$1
    local url=$2

    echo "Downloading $planet texture..."
    if curl -L "$url" -o "$planet/albedo.png"; then
        echo "$planet texture downloaded successfully"
    else
        echo "Failed to download $planet texture"
    fi
}

# Mercury - from Solar System Scope
download_texture "mercury" \
    "https://www.solarsystemscope.com/textures/download/2k/mercury.jpg"

# Venus - from Solar System Scope
download_texture "venus" \
    "https://www.solarsystemscope.com/textures/download/2k/venus.jpg"

# Earth - from Solar System Scope (without clouds for base texture)
download_texture "earth" \
    "https://www.solarsystemscope.com/textures/download/2k/earth.jpg"

# Mars - from Solar System Scope
download_texture "mars" \
    "https://www.solarsystemscope.com/textures/download/2k/mars.jpg"

# Jupiter - from Solar System Scope
download_texture "jupiter" \
    "https://www.solarsystemscope.com/textures/download/2k/jupiter.jpg"

# Saturn - from Solar System Scope
download_texture "saturn" \
    "https://www.solarsystemscope.com/textures/download/2k/saturn.jpg"

# Uranus - from Solar System Scope
download_texture "uranus" \
    "https://www.solarsystemscope.com/textures/download/2k/uranus.jpg"

# Neptune - from Solar System Scope
download_texture "neptune" \
    "https://www.solarsystemscope.com/textures/download/2k/neptune.jpg"

# Moon - from Solar System Scope
download_texture "moon" \
    "https://www.solarsystemscope.com/textures/download/2k/moon.jpg"

# Major moons - Io, Europa, Ganymede, Callisto
download_texture "io" \
    "https://www.solarsystemscope.com/textures/download/2k/io.jpg"

download_texture "europa" \
    "https://www.solarsystemscope.com/textures/download/2k/europa.jpg"

download_texture "ganymede" \
    "https://www.solarsystemscope.com/textures/download/2k/ganymede.jpg"

download_texture "callisto" \
    "https://www.solarsystemscope.com/textures/download/2k/callisto.jpg"

download_texture "titan" \
    "https://www.solarsystemscope.com/textures/download/2k/titan.jpg"

download_texture "triton" \
    "https://www.solarsystemscope.com/textures/download/2k/triton.jpg"

# Additional moons
download_texture "phobos" \
    "https://www.solarsystemscope.com/textures/download/2k/phobos.jpg"

download_texture "deimos" \
    "https://www.solarsystemscope.com/textures/download/2k/deimos.jpg"

download_texture "enceladus" \
    "https://www.solarsystemscope.com/textures/download/2k/enceladus.jpg"

download_texture "mimas" \
    "https://www.solarsystemscope.com/textures/download/2k/mimas.jpg"

echo ""
echo "All planetary textures downloaded!"
echo ""
echo "Note: These textures are from Solar System Scope and are licensed appropriately for educational use."
echo "They are already in equirectangular projection and suitable for Bevy UV spheres."
echo ""
echo "The Sun will remain emissive without texture as it represents a star."
echo ""
echo "Next step: Update the Bevy code to load these textures."