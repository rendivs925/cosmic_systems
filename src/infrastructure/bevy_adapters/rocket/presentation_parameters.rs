//! Pure, bounded mappings from authoritative Rocket inputs to presentation controls.
//!
//! These values are render and audio controls only. They do not model thrust,
//! atmosphere, terrain contact, or heating and never feed back into simulation.

const SEA_LEVEL_PRESSURE_PA: f64 = 101_325.0;
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
const GROUND_EFFECT_FADE_ALTITUDE_M: f64 = 50.0;
const SHOCK_MIN_DYNAMIC_PRESSURE_PA: f64 = 5_000.0;
const SHOCK_FULL_DYNAMIC_PRESSURE_PA: f64 = 50_000.0;
const HEATING_VISIBLE_HEAT_FLUX_W_M2: f64 = 10_000.0;
const EXTERNAL_AUDIO_REFERENCE_DISTANCE_M: f64 = 1_000.0;

/// Numeric inputs sampled from authoritative state by a future presentation system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RocketPresentationInputs {
    pub throttle_unit: f64,
    pub thrust_fraction_unit: f64,
    pub ignition_elapsed_s: f64,
    pub ignition_ramp_duration_s: f64,
    pub ambient_pressure_pa: f64,
    pub density_kg_m3: f64,
    pub terrain_distance_m: f64,
    pub mach_number: f64,
    pub dynamic_pressure_pa: f64,
    pub total_heat_flux_w_m2: f64,
    pub observer_distance_m: f64,
}

/// Bounded render/audio controls derived from [`RocketPresentationInputs`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RocketPresentationParameters {
    pub plume_intensity_unit: f64,
    pub plume_expansion_ratio: f64,
    pub ignition_intensity_unit: f64,
    pub ground_effect_intensity_unit: f64,
    pub shock_intensity_unit: f64,
    pub heating_intensity_unit: f64,
    pub external_audio_attenuation_unit: f64,
}

/// Maps authoritative numeric inputs to finite, bounded presentation controls.
pub fn map_presentation_parameters(
    inputs: RocketPresentationInputs,
) -> RocketPresentationParameters {
    let throttle_unit = unit_interval(inputs.throttle_unit);
    let thrust_fraction_unit = unit_interval(inputs.thrust_fraction_unit);
    let plume_intensity_unit = throttle_unit * thrust_fraction_unit;
    let ignition_intensity_unit = plume_intensity_unit
        * ramp_unit(inputs.ignition_elapsed_s, inputs.ignition_ramp_duration_s);
    let pressure_fraction_unit = nonnegative(inputs.ambient_pressure_pa) / SEA_LEVEL_PRESSURE_PA;
    let plume_expansion_ratio = 1.0 + (1.0 - unit_interval(pressure_fraction_unit)) * 2.0;
    let ground_effect_intensity_unit = plume_intensity_unit
        * (1.0
            - unit_interval(
                nonnegative(inputs.terrain_distance_m) / GROUND_EFFECT_FADE_ALTITUDE_M,
            ));
    let shock_mach_unit = ramp_unit(inputs.mach_number - 1.0, 0.5);
    let shock_pressure_unit = ramp_between_unit(
        inputs.dynamic_pressure_pa,
        SHOCK_MIN_DYNAMIC_PRESSURE_PA,
        SHOCK_FULL_DYNAMIC_PRESSURE_PA,
    );
    let heating_intensity_unit =
        ramp_unit(inputs.total_heat_flux_w_m2, HEATING_VISIBLE_HEAT_FLUX_W_M2);
    let atmospheric_audio_unit =
        unit_interval(nonnegative(inputs.density_kg_m3) / SEA_LEVEL_DENSITY_KG_M3);
    let distance_audio_unit = 1.0
        / (1.0
            + (nonnegative(inputs.observer_distance_m) / EXTERNAL_AUDIO_REFERENCE_DISTANCE_M)
                .powi(2));

    RocketPresentationParameters {
        plume_intensity_unit,
        plume_expansion_ratio: finite_in_range(plume_expansion_ratio, 1.0, 3.0),
        ignition_intensity_unit,
        ground_effect_intensity_unit,
        shock_intensity_unit: shock_mach_unit * shock_pressure_unit,
        heating_intensity_unit,
        external_audio_attenuation_unit: atmospheric_audio_unit * distance_audio_unit,
    }
}

fn nonnegative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn unit_interval(value: f64) -> f64 {
    finite_in_range(value, 0.0, 1.0)
}

fn finite_in_range(value: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}

fn ramp_unit(value: f64, full_scale: f64) -> f64 {
    if !full_scale.is_finite() || full_scale <= 0.0 {
        return 0.0;
    }
    unit_interval(nonnegative(value) / full_scale)
}

fn ramp_between_unit(value: f64, start: f64, end: f64) -> f64 {
    ramp_unit(nonnegative(value) - start, end - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bounded_and_finite(parameters: RocketPresentationParameters) {
        assert!((0.0..=1.0).contains(&parameters.plume_intensity_unit));
        assert!((1.0..=3.0).contains(&parameters.plume_expansion_ratio));
        assert!((0.0..=1.0).contains(&parameters.ignition_intensity_unit));
        assert!((0.0..=1.0).contains(&parameters.ground_effect_intensity_unit));
        assert!((0.0..=1.0).contains(&parameters.shock_intensity_unit));
        assert!((0.0..=1.0).contains(&parameters.heating_intensity_unit));
        assert!((0.0..=1.0).contains(&parameters.external_audio_attenuation_unit));
    }

    #[test]
    fn maps_sea_level_ignition_with_terrain_ground_effect() {
        let parameters = map_presentation_parameters(RocketPresentationInputs {
            throttle_unit: 1.0,
            thrust_fraction_unit: 1.0,
            ignition_elapsed_s: 0.5,
            ignition_ramp_duration_s: 1.0,
            ambient_pressure_pa: SEA_LEVEL_PRESSURE_PA,
            density_kg_m3: SEA_LEVEL_DENSITY_KG_M3,
            terrain_distance_m: 0.0,
            mach_number: 0.0,
            dynamic_pressure_pa: 0.0,
            total_heat_flux_w_m2: 0.0,
            observer_distance_m: 0.0,
        });

        assert_bounded_and_finite(parameters);
        assert_eq!(parameters.plume_expansion_ratio, 1.0);
        assert_eq!(parameters.ignition_intensity_unit, 0.5);
        assert_eq!(parameters.ground_effect_intensity_unit, 1.0);
        assert_eq!(parameters.shock_intensity_unit, 0.0);
    }

    #[test]
    fn maps_max_q_to_bounded_shock_and_heating() {
        let parameters = map_presentation_parameters(RocketPresentationInputs {
            throttle_unit: 1.0,
            thrust_fraction_unit: 1.0,
            ignition_elapsed_s: 2.0,
            ignition_ramp_duration_s: 1.0,
            ambient_pressure_pa: 30_000.0,
            density_kg_m3: 0.5,
            terrain_distance_m: 1_000.0,
            mach_number: 1.5,
            dynamic_pressure_pa: SHOCK_FULL_DYNAMIC_PRESSURE_PA,
            total_heat_flux_w_m2: HEATING_VISIBLE_HEAT_FLUX_W_M2,
            observer_distance_m: 1_000.0,
        });

        assert_bounded_and_finite(parameters);
        assert_eq!(parameters.shock_intensity_unit, 1.0);
        assert_eq!(parameters.heating_intensity_unit, 1.0);
        assert_eq!(parameters.ground_effect_intensity_unit, 0.0);
    }

    #[test]
    fn maps_thin_atmosphere_continuously() {
        let sea_level = map_presentation_parameters(RocketPresentationInputs {
            ambient_pressure_pa: SEA_LEVEL_PRESSURE_PA,
            density_kg_m3: SEA_LEVEL_DENSITY_KG_M3,
            observer_distance_m: 100.0,
            ..inactive_inputs()
        });
        let thin_atmosphere = map_presentation_parameters(RocketPresentationInputs {
            ambient_pressure_pa: 10_000.0,
            density_kg_m3: 0.1,
            observer_distance_m: 100.0,
            ..inactive_inputs()
        });

        assert_bounded_and_finite(thin_atmosphere);
        assert!(thin_atmosphere.plume_expansion_ratio > sea_level.plume_expansion_ratio);
        assert!(
            thin_atmosphere.external_audio_attenuation_unit
                < sea_level.external_audio_attenuation_unit
        );
    }

    #[test]
    fn maps_vacuum_and_nonfinite_inputs_to_safe_bounds() {
        let parameters = map_presentation_parameters(RocketPresentationInputs {
            ambient_pressure_pa: 0.0,
            density_kg_m3: f64::NAN,
            observer_distance_m: f64::INFINITY,
            ..inactive_inputs()
        });

        assert_bounded_and_finite(parameters);
        assert_eq!(parameters.plume_expansion_ratio, 3.0);
        assert_eq!(parameters.external_audio_attenuation_unit, 0.0);
    }

    fn inactive_inputs() -> RocketPresentationInputs {
        RocketPresentationInputs {
            throttle_unit: 0.0,
            thrust_fraction_unit: 0.0,
            ignition_elapsed_s: 0.0,
            ignition_ramp_duration_s: 1.0,
            ambient_pressure_pa: 0.0,
            density_kg_m3: 0.0,
            terrain_distance_m: 1_000.0,
            mach_number: 0.0,
            dynamic_pressure_pa: 0.0,
            total_heat_flux_w_m2: 0.0,
            observer_distance_m: 0.0,
        }
    }
}
