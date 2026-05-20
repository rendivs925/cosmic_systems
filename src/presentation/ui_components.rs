use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct UiCapture;

#[derive(Resource)]
pub(crate) struct UiRoots {
    pub _navbar: Entity,
    pub _info_card: Entity,
    pub notifications: Entity,
}

#[derive(Component)]
pub(crate) struct NavButton {
    pub name: String,
    pub group: NavGroup,
}

#[derive(Clone, Copy, PartialEq)]
pub enum NavGroup {
    CelestialBody, // Unified for all planets and moons
}

#[derive(Component)]
pub(crate) struct NavButtonLabel;

#[derive(Component)]
pub(crate) struct FpsText;

#[derive(Component)]
pub(crate) struct MenuButton {
    pub action: MenuAction,
    pub primary: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Explore,
    Orbits,
}

#[derive(Component)]
pub(crate) struct SelectorPanelRoot;

#[derive(Resource)]
pub(crate) struct UiMenuState {
    pub selector_open: bool,
    pub info_card_open: bool,
}

impl Default for UiMenuState {
    fn default() -> Self {
        Self {
            selector_open: false,
            info_card_open: false,
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