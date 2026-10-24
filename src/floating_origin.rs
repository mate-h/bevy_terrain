use bevy::{math::DVec3, prelude::*};

#[cfg(feature = "big_space")]
use big_space::{
    floating_origins::BigSpace,
    prelude::{CellCoord, Grids},
};

#[cfg(not(feature = "big_space"))]
pub fn register_plugins(_app: &mut App) {}

#[cfg(feature = "big_space")]
pub fn register_plugins(app: &mut App) {
    use big_space::prelude::BigSpaceDefaultPlugins;
    app.add_plugins(BigSpaceDefaultPlugins);
}

#[cfg(feature = "big_space")]
pub fn view_local_position(
    grids: &Grids,
    view: Entity,
    transform: &Transform,
    cell: &CellCoord,
) -> DVec3 {
    let grid = grids.parent_grid(view).unwrap();
    grid.grid_position_double(cell, transform)
}

#[cfg(not(feature = "big_space"))]
pub fn view_local_position(global_transform: &GlobalTransform) -> DVec3 {
    global_transform.translation().as_dvec3()
}

#[cfg(feature = "big_space")]
pub fn parent_terrain_under_big_space(
    commands: &mut Commands,
    terrain: Entity,
    big_space: &Query<Entity, With<BigSpace>>,
) {
    let root = big_space.single().unwrap();
    commands.entity(root).add_child(terrain);
}

#[cfg(not(feature = "big_space"))]
pub fn parent_terrain_under_big_space(
    _commands: &mut Commands,
    _terrain: Entity,
    _: &Query<Entity>,
) {
}
