# GMAT Reference Reproduction

This directory provides a reproducible two-body GMAT input for independent
comparison. It is not part of runtime simulation, CI, or a claim that GMAT has
validated this repository.

`generate_spiceypy_reference_cases.py` reproduces the checked-in CSPICE/SpiceyPy
orientation, KSC Earth-fixed position, Sun-direction, Earth-GM, and two-body
one-day propagation values. It is likewise validation-only. Install SpiceyPy
outside the repository, provision the pinned kernels, then run from the
repository root:

```sh
PYTHONPATH=/tmp/opencode/spiceypy \
  python3 scripts/scientific_validation/generate_spiceypy_reference_cases.py
```

The CSPICE script uses `pck00011.tpc`, `gm_de440.tpc`, `de440s.bsp`, and
`naif0012.tls`. It does not evaluate the project's J2 or Earth-Moon-Sun
long-arc force tiers; those remain unverified until an approved independent
propagator export is recorded.

1. Install the reviewed GMAT release for the validation run from the official
   [GMAT releases](https://github.com/GMAT/GMAT/releases). Record the release,
   platform package checksum, and `GMAT --version` output with the generated
   reference data.
2. From this directory, run the installed non-interactive executable:

   ```sh
   /absolute/path/to/GMAT two_body_reference.script
   ```

3. Preserve the generated `gmat_two_body_reference.txt` with its GMAT version
   and the script revision. Convert kilometers and kilometers per second to SI
   explicitly before adding a new machine-readable reference case.
4. Compare only the `TwoBody` scenario. The script deliberately omits J2,
   third-body, drag, SRP, and maneuver models, so it cannot validate the
   `EarthJ2`, `EarthMoonSun`, or powered-flight tiers.

The initial epoch is UTC `01 Jan 2000 11:58:55.816`, the UTC representation of
the J2000 dynamical epoch used by the local TDB reference case. The force model,
integrator settings, output units, and coordinate system are declared in the
script. Review them against the intended external case before treating any output
as an acceptance result.
