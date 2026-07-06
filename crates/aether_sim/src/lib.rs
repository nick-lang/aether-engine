//! Gameplay systems for package-driven scenes.

use aether_ecs::{Collectible, PackageEntityId, Player, WinCondition, WinProgress};
use bevy::prelude::*;

pub const DEFAULT_TICK_HZ: f64 = 60.0;

pub struct AetherSimPlugin;

impl Plugin for AetherSimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(DEFAULT_TICK_HZ))
            .add_systems(
                FixedUpdate,
                (
                    player_movement,
                    collectible_pickup,
                    evaluate_win_condition,
                )
                    .chain(),
            );
    }
}

fn player_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Fixed>>,
    mut query: Query<(&Player, &mut Transform), With<Player>>,
) {
    let Some((player, mut transform)) = query.iter_mut().next() else {
        return;
    };

    let mut direction = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }

    if direction != Vec2::ZERO {
        direction = direction.normalize();
        transform.translation +=
            (direction * player.speed * time.delta_secs()).extend(0.0);
    }
}

fn collectible_pickup(
    mut progress: ResMut<WinProgress>,
    player: Query<&Transform, With<Player>>,
    collectibles: Query<(Entity, &PackageEntityId, &Collectible, &Transform)>,
    mut commands: Commands,
) {
    let Some(player_tf) = player.iter().next() else {
        return;
    };

    for (entity, entity_id, collectible, transform) in &collectibles {
        if progress.picked_entities.contains(&entity_id.0) {
            continue;
        }
        let distance = player_tf
            .translation
            .truncate()
            .distance(transform.translation.truncate());
        if distance < 26.0 {
            progress.picked_entities.insert(entity_id.0.clone());
            if !progress.collected.contains(&collectible.id) {
                progress.collected.push(collectible.id.clone());
            }
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn evaluate_win_condition(mut progress: ResMut<WinProgress>, conditions: Query<&WinCondition>) {
    if progress.won {
        return;
    }
    for condition in &conditions {
        let required = &condition.requires_collectibles;
        if required.is_empty() {
            continue;
        }
        if required
            .iter()
            .all(|id| progress.collected.iter().any(|c| c == id))
        {
            progress.won = true;
            break;
        }
    }
}
