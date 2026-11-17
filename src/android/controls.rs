//! Mobile interaction layer.
//!
//! Android has no keyboard or mouse, so every desktop binding needs a touch
//! equivalent. Rather than duplicating each mode's control logic, the on-screen
//! panel synthesises the same `ButtonInput<KeyCode>` presses that the desktop
//! bindings already consume. One finger drag/pinch remains camera control.
//!
//! The launch mode is selected with an intent extra:
//!   am start -n com.rendivs925.cosmicsystems/android.app.NativeActivity \
//!       --es mode rocket

use bevy::input::ButtonInput;
use bevy::prelude::*;
use bevy::ui::BorderColor;

use crate::android::diagnostics;
use crate::android::theme::*;
use crate::application::modes::Mode;
use crate::infrastructure::bevy_adapters::input_systems::handle_mouse_planet_selection;

/// Height of the on-screen panel in logical pixels, measured after layout so
/// camera drags can be excluded precisely instead of using a fixed fraction.
#[derive(Resource, Default)]
pub struct ControlPanelExtent(pub f32);

#[derive(Resource, Clone, Copy)]
pub struct MobileMode(pub Mode);

#[derive(Component, Clone, Copy)]
struct MobileKey(KeyCode);

#[derive(Resource, Default)]
struct HeldKeys(Vec<KeyCode>);

#[derive(Component)]
struct ControlsPanel;

#[derive(Component)]
struct ControlsButton;

pub fn install(app: &mut App, mode: Mode) {
    app.insert_resource(MobileMode(mode))
        .insert_resource(HeldKeys::default())
        .insert_resource(ControlPanelExtent::default())
        .add_systems(Startup, spawn_panel)
        .add_systems(Update, (sync_keys, tint_buttons, measure_panel).chain())
        // Taps must be seen by the planet-selection raycast in the same frame.
        .add_systems(Update, tap_select.before(handle_mouse_planet_selection));
}

/// Run mode requested by the launch intent (`--es mode rocket`), default solar.
pub fn launch_mode() -> Mode {
    match diagnostics::intent_extra("mode").as_deref() {
        Some("rocket") => Mode::Rocket,
        Some("craft") => Mode::Craft,
        Some("solar") | None => Mode::Solar,
        Some(other) => {
            diagnostics::log(&format!(
                "controls: unknown launch mode '{other}'; using solar"
            ));
            Mode::Solar
        }
    }
}

fn bindings(mode: Mode) -> Vec<(KeyCode, &'static str)> {
    match mode {
        Mode::Solar => vec![
            (KeyCode::KeyW, "Fwd"),
            (KeyCode::KeyS, "Back"),
            (KeyCode::KeyA, "Left"),
            (KeyCode::KeyD, "Right"),
            (KeyCode::Space, "Up"),
            (KeyCode::ControlLeft, "Down"),
            (KeyCode::Tab, "Next"),
            (KeyCode::Escape, "Clear"),
            (KeyCode::Comma, "Time-"),
            (KeyCode::Period, "Time+"),
        ],
        Mode::Craft => vec![
            (KeyCode::KeyW, "Fwd"),
            (KeyCode::KeyS, "Back"),
            (KeyCode::KeyA, "Left"),
            (KeyCode::KeyD, "Right"),
            (KeyCode::Space, "Up"),
            (KeyCode::ControlLeft, "Down"),
            (KeyCode::KeyQ, "Yaw+"),
            (KeyCode::KeyE, "Yaw-"),
            (KeyCode::KeyH, "Pitch+"),
            (KeyCode::KeyL, "Pitch-"),
            (KeyCode::KeyJ, "Roll+"),
            (KeyCode::KeyK, "Roll-"),
            (KeyCode::ShiftLeft, "Boost"),
            (KeyCode::KeyV, "Cam"),
            (KeyCode::KeyB, "Reset"),
            (KeyCode::Comma, "Time-"),
            (KeyCode::Period, "Time+"),
            (KeyCode::Escape, "Menu"),
        ],
        Mode::Rocket => vec![
            (KeyCode::Space, "Stage"),
            (KeyCode::KeyR, "Restart"),
            (KeyCode::KeyC, "Cam"),
            (KeyCode::KeyH, "HUD"),
            (KeyCode::KeyL, "Log"),
            (KeyCode::Digit1, "Cam1"),
            (KeyCode::Digit2, "Cam2"),
            (KeyCode::Digit3, "Cam3"),
            (KeyCode::Comma, "Time-"),
            (KeyCode::Period, "Time+"),
            (KeyCode::Escape, "Menu"),
        ],
    }
}

fn spawn_panel(mut commands: Commands, mode: Res<MobileMode>, asset_server: Res<AssetServer>) {
    let ui_font: Handle<Font> = asset_server.load(FONT_PATH);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0),
                left: px(0),
                width: percent(100),
                max_height: percent(45),
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(SPACE_2),
                column_gap: px(SPACE_2),
                padding: UiRect::new(
                    px(SPACE_3),
                    px(SPACE_3),
                    px(SPACE_3),
                    px(SPACE_3 + SAFE_BOTTOM),
                ),
                border: UiRect::top(px(1)),
                ..default()
            },
            BackgroundColor(SURFACE),
            BorderColor::all(TRACK),
            ControlsPanel,
        ))
        .with_children(|panel| {
            for (key, label) in bindings(mode.0) {
                panel
                    .spawn((
                        Button,
                        MobileKey(key),
                        ControlsButton,
                        Node {
                            width: px(TOUCH_WIDTH),
                            height: px(TOUCH_HEIGHT),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(SURFACE_RAISED),
                        BorderRadius::all(px(RADIUS_SM)),
                    ))
                    .with_child((
                        Text::new(label),
                        TextFont {
                            font: ui_font.clone(),
                            font_size: FONT_CAPTION,
                            ..default()
                        },
                        TextColor(TEXT_PRIMARY),
                        TextLayout::new_with_justify(Justify::Center),
                    ));
            }
        });
}

fn sync_keys(
    buttons: Query<(&MobileKey, &Interaction), Changed<Interaction>>,
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut held: ResMut<HeldKeys>,
) {
    for (key, interaction) in &buttons {
        let down = matches!(interaction, Interaction::Pressed);
        if down {
            if !held.0.contains(&key.0) {
                keyboard.press(key.0);
                held.0.push(key.0);
            }
        } else if let Some(index) = held.0.iter().position(|held| *held == key.0) {
            keyboard.release(key.0);
            held.0.remove(index);
        }
    }
}

fn tint_buttons(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<ControlsButton>),
    >,
) {
    for (interaction, mut color) in &mut buttons {
        *color = match *interaction {
            Interaction::Pressed => BackgroundColor(SURFACE_PRESSED),
            Interaction::Hovered => BackgroundColor(SURFACE_PRESSED),
            Interaction::None => BackgroundColor(SURFACE_RAISED),
        };
    }
}

/// Publish the laid-out panel height so camera drags can ignore that region.
fn measure_panel(
    panel: Query<&ComputedNode, With<ControlsPanel>>,
    mut extent: ResMut<ControlPanelExtent>,
) {
    if let Ok(node) = panel.single() {
        extent.0 = node.size().y * node.inverse_scale_factor();
    }
}

/// A short touch that starts above the control panel selects the body under it,
/// reusing the desktop mouse-selection raycast by synthesising one click.
fn tap_select(
    touches: Res<Touches>,
    extent: Res<ControlPanelExtent>,
    mut windows: Query<&mut Window>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let panel_top = if extent.0 > 0.0 {
        window.height() - extent.0
    } else {
        window.height() * 0.55
    };
    for touch in touches.iter_just_released() {
        let travel = touch.position().distance(touch.start_position());
        if travel > 24.0 || touch.start_position().y > panel_top {
            continue;
        }
        window.set_cursor_position(Some(touch.position()));
        // just_pressed is visible to consumers for this frame even after release.
        mouse.press(MouseButton::Left);
        mouse.release(MouseButton::Left);
    }
}
