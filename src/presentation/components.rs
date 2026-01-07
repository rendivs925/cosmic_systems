use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct UiCapture;

#[derive(Resource)]
pub(crate) struct UiRoots {
    _navbar: Entity,
    _info_card: Entity,
    notifications: Entity,
}

#[derive(Component)]
pub(crate) struct NavButton {
    name: String,
    group: NavGroup,
}

#[derive(Clone, Copy, PartialEq)]
enum NavGroup {
    CelestialBody, // Unified for all planets and moons
}

#[derive(Component)]
pub(crate) struct NavButtonLabel;

#[derive(Component)]
pub(crate) struct FpsText;

#[derive(Component)]
pub(crate) struct MenuButton {
    action: MenuAction,
    primary: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    Explore,
    Orbits,
}

#[derive(Component)]
pub(crate) struct SelectorPanelRoot;

#[derive(Resource)]
pub(crate) struct UiMenuState {
    selector_open: bool,
    info_card_open: bool,
}

impl Default for UiMenuState {
    fn default() -> Self {
        Self {
            selector_open: false,
            info_card_open: true,
        }
    }
}

#[derive(Component)]
pub(crate) struct InfoCardRoot;

#[derive(Component)]
pub(crate) struct InfoCardTitle;

#[derive(Component)]
pub(crate) struct InfoCardSubtitle;

#[derive(Component)]
pub(crate) struct InfoCardBody;

#[derive(Component)]
pub(crate) struct InfoCardToggleButton;

#[derive(Component)]
pub(crate) struct InfoCardExternalToggle;

#[derive(Component)]
pub(crate) struct NotificationLayer;

#[derive(Component)]
pub(crate) struct NotificationUi;