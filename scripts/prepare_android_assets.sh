#!/usr/bin/env sh
# Build `assets-android/`: hardlinks to the repository assets, except the five
# 8192x4096 planet albedos, which are downscaled to 4096x2048 for mobile GPUs.
#
# Why: those five PNGs decode to 5 x 134 MB of RGBA and dominate APK size,
# texture upload time and GPU memory on a phone. The desktop build keeps the
# originals because it packages `assets/`.
#
# Re-run after adding or removing assets. `assets-android/` is generated and
# gitignored.
set -eu

cd "$(dirname "$0")/.."

SRC=assets
DST=assets-android

rm -rf "$DST"
mkdir -p "$DST"

# Copy the tree. Hardlinks are denied by Android's SELinux policy in app data,
# and symlinks are not guaranteed to be followed by aapt2.
cp -R "$SRC/." "$DST/"

# Replace the oversized albedos with mobile-sized copies (breaks the hardlink,
# leaving the originals untouched).
for file in \
    textures/planets/earth/albedo.png \
    textures/planets/moon/albedo.png \
    textures/planets/mercury/albedo.png \
    textures/planets/mars/albedo.png \
    textures/planets/venus/albedo.png
do
    [ -f "$SRC/$file" ] || continue
    rm -f "$DST/$file"
    magick "$SRC/$file" -resize 4096x2048! -strip -define png:compression-level=9 "$DST/$file"
done

printf 'assets-android prepared: %s files\n' "$(find "$DST" -type f | wc -l)"
