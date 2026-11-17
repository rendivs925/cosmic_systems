//! Mobile interaction layer.
//!
//! Android has no keyboard or mouse, so every desktop binding needs a touch
//! equivalent. Rather than duplicating each mode's control logic, the on-screen
//! panel synthesises the same `ButtonInput<KeyCode>` presses that the desktop
//! bindings already consume. One finger drag/pinch remains camera control.
//!
//! The launch mode is selected with an intent extra:
//!   adb shell am start -n com.rendivs925.cosmicsystems/android.app.NativeActivity \
//!       --es mode rocket

use bevy::input::ButtonInput;
use bevy::prelude::*;

use crate::android::diagnostics;
use crate::application::modes::Mode;
use crate::infrastructure::bevy_adapters::input_systems::handle_mouse_planet_selection;

/// Fraction of the window height occupied by the on-screen panel. Camera drags
/// that start below this line are ignored so buttons do not also rotate the view.
pub const PANEL_HEIGHT_FRACTION: f32 = 0.45;

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
        .add_systems(Startup, spawn_panel)
        .add_systems(Update, (sync_keys, tint_buttons).chain())
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

fn spawn_panel(mut commands: Commands, mode: Res<MobileMode>) {
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
                row_gap: px(6),
                column_gap: px(6),
                padding: UiRect::all(px(6)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.03, 0.05, 0.78)),
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
                            width: px(66),
                            height: px(50),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.16, 0.17, 0.22, 0.95)),
                    ))
                    .with_child((
                        Text::new(label),
                        TextFont {
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
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
            Interaction::Pressed => BackgroundColor(Color::srgba(0.30, 0.55, 0.85, 0.98)),
            Interaction::Hovered => BackgroundColor(Color::srgba(0.22, 0.25, 0.33, 0.98)),
            Interaction::None => BackgroundColor(Color::srgba(0.16, 0.17, 0.22, 0.95)),
        };
    }
}

/// A short touch that starts above the control panel selects the body under it,
/// reusing the desktop mouse-selection raycast by synthesising one click.
fn tap_select(
    touches: Res<Touches>,
    mut windows: Query<&mut Window>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let panel_top = window.height() * (1.0 - PANEL_HEIGHT_FRACTION);
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
