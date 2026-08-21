## Why

The existing rocket physics is not real dynamics: it integrates `a = thrust/mass` with no gravity, and rotates the vehicle via `Quat::from_vec4(angular_velocity.extend(0.0)) * dt`, which is not a valid quaternion rotation. There is no inertia tensor, no torque model, no angular acceleration, and no center of mass. A physically authoritative rocket requires proper 6-DOF rigid-body dynamics where physics determines the trajectory rather than faking flight by manipulating the transform.

## What Changes

- Replace the toy integration in `rocket_systems.rs` with real 6-DOF rigid-body dynamics.
- Introduce a proper angular state model: orientation `Quat`, angular velocity, angular acceleration, inertia tensor, and center of mass.
- Fix quaternion integration using `Quat::from_scaled_axis(angular_velocity * dt)` with normalization.
- Accumulate forces (translation) and torques (rotation) through separate, ordered systems.
- Extend `RocketComponent` into a full physical state (reusing the existing fields where they already fit).
- Ensure physics is the sole writer of rocket motion; `Transform` is derived, never directly faked.

## Capabilities

### New Capabilities

- `rocket-dynamics`: 6-DOF rigid-body dynamics for the rocket: translation (position/velocity/acceleration/mass) and rotation (orientation/angular velocity/angular acceleration/inertia tensor) driven by accumulated forces and torques.

### Modified Capabilities

<!-- None - no existing openspec/specs exist yet. -->