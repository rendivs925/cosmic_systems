//! Rocket-mode game-flow states.
//!
//! These states orchestrate the user-facing lifecycle (menu through debrief).
//! They are global application flow only: mission phase and physical state stay
//! per-vehicle entity/domain data, and no state here changes simulation
//! authority. The state is registered only by rocket-mode composition so
//! normal and craft modes never see it.

use bevy::prelude::*;

/// Ordered rocket-mode game flow. `Title` is the default entry state.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RocketGameFlow {
    #[default]
    Title,
    Profile,
    Assembly,
    Briefing,
    Flight,
    Pause,
    Debrief,
}

/// Registers the rocket game-flow state. Added only by `RocketModePlugin`.
pub struct RocketGameFlowPlugin;

impl Plugin for RocketGameFlowPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<RocketGameFlow>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    #[test]
    fn plugin_initializes_the_title_state() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, RocketGameFlowPlugin));
        app.update();
        assert_eq!(
            *app.world().resource::<State<RocketGameFlow>>().get(),
            RocketGameFlow::Title
        );
    }

    #[test]
    fn state_transitions_between_game_flow_phases() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, RocketGameFlowPlugin));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<RocketGameFlow>>()
            .set(RocketGameFlow::Assembly);
        app.update();
        assert_eq!(
            *app.world().resource::<State<RocketGameFlow>>().get(),
            RocketGameFlow::Assembly
        );
    }
}
