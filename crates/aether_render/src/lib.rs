//! 2D render setup (camera, HUD).

use aether_ecs::{Player, WinUiText};
use bevy::prelude::*;

pub struct AetherRenderPlugin;

impl Plugin for AetherRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (camera_follow_player, draw_hud));
    }
}

#[derive(Component)]
struct HudLabel;

#[derive(Component)]
struct MainCamera;

fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, MainCamera));
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                HudLabel,
                Text::new("Find the glowing shard (WASD to move)"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.94, 0.98)),
            ));
        });
}

/// Keep the player at the center of the view; world coordinates stay absolute in data.
fn camera_follow_player(
    player: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    mut camera: Query<&mut Transform, (With<MainCamera>, Without<Player>)>,
) {
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let Ok(mut cam) = camera.get_single_mut() else {
        return;
    };
    cam.translation.x = player_tf.translation.x;
    cam.translation.y = player_tf.translation.y;
}

fn draw_hud(text: Res<WinUiText>, mut query: Query<&mut Text, With<HudLabel>>) {
    let Some(mut label) = query.iter_mut().next() else {
        return;
    };
    *label = Text::new(text.0.clone());
}
