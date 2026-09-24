//! Flight-recorder CSV export to the local `exports/` directory.

use super::recorder::FlightRecorder;
use crate::infrastructure::bevy_adapters::entity_components::Selectable;
use bevy::prelude::*;
use std::fmt::Write as _;

/// Directory receiving exported flight recordings, relative to the working
/// directory. Created on demand.
pub const FLIGHT_EXPORT_DIR: &str = "exports";

/// One vehicle's recording serialized as CSV: a header row per recorded
/// field plus a `#`-prefixed notable-events section. Pure function so the
/// format is testable without touching the filesystem.
pub fn flight_recorder_csv(name: &str, recorder: &FlightRecorder) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "vehicle,time_s,alt_agl_m,alt_msl_m,speed_mps,mach,q_pa,g_load,thrust_n,throttle,phase,stage,propellant_fraction,apoapsis_m,periapsis_m,blackout,drogue,main"
    );
    for e in recorder.entries() {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{:.4},{},{},{},{},{}",
            name,
            e.time_s,
            e.altitude_agl_m,
            e.altitude_msl_m,
            e.velocity_total_mps,
            e.mach_number,
            e.dynamic_pressure_pa,
            e.g_load,
            e.total_thrust_n,
            e.throttle,
            format!("{:?}", e.mission_phase).as_str(),
            e.active_stage,
            e.propellant_fraction,
            e.apoapsis_altitude_m,
            e.periapsis_altitude_m,
            e.plasma_blackout,
            e.drogue_deployed,
            e.main_deployed,
        );
    }
    for ev in recorder.events() {
        let _ = writeln!(out, "#event,{},{},{}", ev.time_s, name, ev.label);
    }
    out
}

/// Make a vehicle name safe for a filename component.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// F11 dumps every vehicle's ring-buffer contents and notable events to
/// `exports/flight_<stamp>_<vehicle>.csv`. IO problems are logged, never
/// fatal (AGENTS.md section 38): the recording itself is untouched.
pub fn handle_flight_recorder_export_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut rocket_query: Query<(Entity, &Selectable, &mut FlightRecorder)>,
) {
    if !keyboard.just_pressed(KeyCode::F11) {
        return;
    }

    // Guard: an unwritable export directory disables the feature cleanly.
    if let Err(e) = std::fs::create_dir_all(FLIGHT_EXPORT_DIR) {
        bevy::log::warn!(
            "Flight export disabled: cannot create {}: {e}",
            FLIGHT_EXPORT_DIR
        );
        return;
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    for (entity, selectable, recorder) in rocket_query.iter_mut() {
        let path = std::path::Path::new(FLIGHT_EXPORT_DIR).join(format!(
            "flight_{stamp}_{}_{}.csv",
            sanitize_filename(&selectable.name),
            entity.index()
        ));
        let csv = flight_recorder_csv(&selectable.name, &recorder);
        match std::fs::write(&path, csv) {
            Ok(()) => bevy::log::info!(
                "Flight recording exported: {} ({} entries, {} events)",
                path.display(),
                recorder.entries().len(),
                recorder.events().len()
            ),
            Err(e) => bevy::log::warn!("Flight export failed ({}): {e}", path.display()),
        }
    }
}
