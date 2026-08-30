#!/usr/bin/env python3
"""Print the CSPICE values recorded in reference_cases_v1.ron.

This is a validation-only reproduction tool. Install SpiceyPy outside the
repository, then run it from the repository root with the pinned kernels
provisioned. It neither participates in runtime simulation nor updates cases.
"""

import math

import spiceypy as spice


KERNEL_ROOT = "assets/large_files/kernels/de440"


def load(*kernel_names):
    spice.kclear()
    for kernel_name in kernel_names:
        spice.furnsh(f"{KERNEL_ROOT}/{kernel_name}")


def values(items):
    return " ".join(format(value, ".17g") for value in items)


def main():
    load("naif0012.tls", "pck00011.tpc", "gm_de440.tpc", "de440s.bsp")
    epoch = spice.str2et("JD 2451545.0 TDB")

    rotation, angular_velocity = spice.xf2rav(spice.sxform("J2000", "IAU_EARTH", epoch))
    print("earth-orientation-j2000-cspice quaternion_wxyz", values(spice.m2q(rotation)))
    print("earth-orientation-j2000-cspice angular_velocity_rad_s", values(angular_velocity))

    radii_km = spice.bodvrd("EARTH", "RADII", 3)[1]
    flattening = (radii_km[0] - radii_km[2]) / radii_km[0]
    ksc_position_km = spice.georec(
        math.radians(-80.6480), math.radians(28.5721), 0.003, radii_km[0], flattening
    )
    print("ksc-earth-fixed-cspice position_km", values(ksc_position_km))

    sun_state_km_kmps, _ = spice.spkezr("SUN", epoch, "J2000", "NONE", "EARTH")
    print("sun-from-earth-j2000-cspice direction", values(spice.vhat(sun_state_km_kmps[:3])))

    earth_gm_km3_s2 = spice.bodvrd("EARTH", "GM", 1)[1][0]
    print("earth-two-body-7000km-cspice gm_km3_s2", format(earth_gm_km3_s2, ".17g"))
    state_km_kmps = [7000.0, 0.0, 0.0, 0.0, math.sqrt(earth_gm_km3_s2 / 7000.0), 0.0]
    propagated_km_kmps = spice.conics(
        spice.oscelt(state_km_kmps, epoch, earth_gm_km3_s2), epoch + 86_400.0
    )
    print("earth-two-body-leo-one-day-cspice state_km_kmps", values(propagated_km_kmps))


if __name__ == "__main__":
    main()
