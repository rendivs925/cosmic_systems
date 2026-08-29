#!/usr/bin/env sh
set -eu

kernel_dir="assets/large_files/kernels/de440"
mkdir -p "$kernel_dir"

curl --fail --location --output "$kernel_dir/de440s.bsp" \
    "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/de440s.bsp"
curl --fail --location --output "$kernel_dir/pck00011.tpc" \
    "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/pck00011.tpc"
curl --fail --location --output "$kernel_dir/gm_de440.tpc" \
    "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/gm_de440.tpc"
curl --fail --location --output "$kernel_dir/naif0012.tls" \
    "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/lsk/naif0012.tls"

# Runtime manifest validation verifies the pinned sizes and SHA-256 checksums.
# Run the ignored DE440 integration regression after provisioning.
cargo test de440_earth_state_matches_recorded_horizons_reference_at_j2000 -- --ignored
