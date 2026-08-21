## 1. Atmosphere model

- [x] 1.1 Add `AtmosphereSource` trait returning temperature, pressure, density, speed of sound by altitude
- [x] 1.2 Implement `EarthAtmosphere` (exponential/ISA-style reference) and `VacuumAtmosphere`
- [x] 1.3 Register an atmosphere source per planet (Earth real, Moon/vacuum)
- [x] 1.4 Unit tests: Earth density/pressure/temp/speed-of-sound vs altitude, per-planet difference, vacuum zero-density

## 2. Aerodynamics domain logic

- [x] 2.1 Add dynamic pressure `q = ½ρv²` and Mach number calculation
- [x] 2.2 Add angle-of-attack calculation from body axis vs velocity
- [x] 2.3 Add drag, lift, and side force from q, coefficients, and reference area
- [x] 2.4 Add center of pressure from geometry and aerodynamic torque about center of mass
- [x] 2.5 Add Max Q tracking
- [x] 2.6 Unit tests: drag = q·Cd·A opposing velocity, AoA computation, q/Mach values, CoP offset torque, Max Q peak

## 3. Atmosphere/aero systems

- [x] 3.1 Add `atmosphere_properties` system exposing altitude → atmosphere for the aero and propulsion systems
- [x] 3.2 Add `aerodynamic_forces` system feeding the translational accumulator
- [x] 3.3 Add `aerodynamic_torque` system feeding the rotational accumulator
- [x] 3.4 Update propulsion ISP selection to consume atmosphere density
- [x] 3.5 Verify no aero/atmosphere system writes the rocket transform directly

## 4. Validation

- [x] 4.1 Run `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo test`
- [x] 4.2 Confirm craft mode unaffected