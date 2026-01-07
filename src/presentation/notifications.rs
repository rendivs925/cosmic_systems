use bevy::prelude::*;
use super::components::*;
use crate::infrastructure::bevy_adapters::components::NotificationQueue;

pub(crate) fn update_notifications_ui(
    mut notifications: ResMut<NotificationQueue>,
    mut commands: Commands,
    roots: Res<UiRoots>,
    children_query: Query<&Children>,
    time: Res<Time>,
    video_state: Res<crate::infrastructure::bevy_adapters::ui_components::VideoRecordingState>,
    mut last_update: Local<f32>,
) {
    let current_time = time.elapsed_secs();

    // Reduce notification update frequency during video recording to prevent UI flickering
    let update_interval = if video_state.is_recording { 0.2 } else { 0.016 }; // 5 FPS during recording, 60 FPS normally

    if current_time - *last_update < update_interval {
        return;
    }
    *last_update = current_time;

    notifications
        .notifications
        .retain(|n| current_time - n.created_at < n.duration);

    // Keep only the most recent few notifications to avoid stacking long lists
    const MAX_NOTIFICATIONS: usize = 3;
    if notifications.notifications.len() > MAX_NOTIFICATIONS {
        let excess = notifications.notifications.len() - MAX_NOTIFICATIONS;
        notifications.notifications.drain(0..excess);
    }

    // Clear any existing notification UI elements before spawning new ones
    if let Ok(children) = children_query.get(roots.notifications) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    commands
        .entity(roots.notifications)
        .with_children(|parent| {
            for notification in notifications.notifications.iter() {
                parent
                    .spawn((
                        Node {
                            border: UiRect::all(Val::Px(1.0)),
                            padding: UiRect::all(Val::Px(10.0)),
                            ..default()
                        },
                        BackgroundColor(notification_color(&notification.notification_type)),
                        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.35)),
                        BorderRadius::all(Val::Px(8.0)),
                        NotificationUi,
                        UiCapture,
                        Interaction::default(),
                    ))
                    .with_children(|row| {
                        let (font, color) = text_style(12.0, Color::srgb(0.95, 0.95, 0.98));
                        row.spawn((Text::new(notification.message.clone()), font, color));
                    });
            }
        });
}

fn text_style(font_size: f32, color: Color) -> (TextFont, TextColor) {
    (
        TextFont {
            font_size,
            ..default()
        },
        TextColor(color),
    )
}

fn notification_color(notification_type: &crate::infrastructure::bevy_adapters::components::NotificationType) -> Color {
    match notification_type {
        crate::infrastructure::bevy_adapters::components::NotificationType::Info => Color::srgb(0.4, 0.6, 0.8),
        crate::infrastructure::bevy_adapters::components::NotificationType::Success => Color::srgb(0.4, 0.8, 0.4),
        crate::infrastructure::bevy_adapters::components::NotificationType::Warning => Color::srgb(0.8, 0.8, 0.4),
        crate::infrastructure::bevy_adapters::components::NotificationType::Error => Color::srgb(0.8, 0.4, 0.4),
    }
}