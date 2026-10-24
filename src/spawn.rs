//! [`SpawnTerrainCommandsExt::spawn_terrain`] queues a terrain and spawns it once the config asset loads.

use crate::{
    floating_origin::parent_terrain_under_big_space,
    plugin::TerrainSettings,
    terrain::TerrainConfig,
    terrain_data::{TileAtlas, TileTree},
    terrain_view::{TerrainViewComponents, TerrainViewConfig},
};
use bevy::{
    camera::visibility::{NoCpuCulling, NoFrustumCulling},
    ecs::system::SystemState,
    prelude::*,
    render::storage::ShaderBuffer,
};
#[cfg(feature = "big_space")]
use big_space::floating_origins::BigSpace;

#[derive(Clone)]
pub(crate) struct TerrainToSpawn<M: Material + Clone> {
    config: Handle<TerrainConfig>,
    view_config: TerrainViewConfig,
    material: M,
    view: Entity,
}

#[derive(Resource)]
pub(crate) struct TerrainsToSpawn<M: Material>(pub(crate) Vec<TerrainToSpawn<M>>);

pub(crate) fn spawn_terrains<M: Material>(
    mut commands: Commands,
    mut terrains: ResMut<TerrainsToSpawn<M>>,
    asset_server: Res<AssetServer>,
) {
    terrains.0.retain(|terrain| {
        if asset_server.is_loaded(&terrain.config) {
            let terrain = terrain.clone();

            commands.queue(move |world: &mut World| {
                let TerrainToSpawn {
                    config,
                    view_config,
                    material,
                    view,
                } = terrain;

                #[cfg(feature = "big_space")]
                type SpawnState<'w, 's, M> = (
                    Commands<'w, 's>,
                    Res<'w, Assets<TerrainConfig>>,
                    Query<'w, 's, Entity, With<BigSpace>>,
                    ResMut<'w, Assets<M>>,
                    ResMut<'w, TerrainViewComponents<TileTree>>,
                    ResMut<'w, Assets<ShaderBuffer>>,
                    Res<'w, TerrainSettings>,
                );

                #[cfg(not(feature = "big_space"))]
                type SpawnState<'w, 's, M> = (
                    Commands<'w, 's>,
                    Res<'w, Assets<TerrainConfig>>,
                    Query<'w, 's, Entity>,
                    ResMut<'w, Assets<M>>,
                    ResMut<'w, TerrainViewComponents<TileTree>>,
                    ResMut<'w, Assets<ShaderBuffer>>,
                    Res<'w, TerrainSettings>,
                );

                let mut state = SystemState::<SpawnState<M>>::new(world);

                let Ok((
                    mut commands,
                    configs,
                    big_space,
                    mut materials,
                    mut tile_trees,
                    mut buffers,
                    settings,
                )) = state.get_mut(world)
                else {
                    return;
                };

                let config = configs.get(config.id()).unwrap().clone();

                let terrain = commands
                    .spawn((
                        config.shape.transform(),
                        TileAtlas::new(&config, &settings),
                        MeshMaterial3d(materials.add(material)),
                        Visibility::Visible,
                        NoCpuCulling,
                        NoFrustumCulling,
                    ))
                    .id();

                parent_terrain_under_big_space(&mut commands, terrain, &big_space);

                tile_trees.insert(
                    (terrain, view),
                    TileTree::new(
                        &config,
                        &view_config,
                        (terrain, view),
                        &mut commands,
                        &mut buffers,
                    ),
                );

                state.apply(world);
            });
            false
        } else {
            true
        }
    });
}

/// Queue a terrain from a loaded [`TerrainConfig`].
pub trait SpawnTerrainCommandsExt<M: Material> {
    /// Spawn `config` for `view` (a camera) once the asset is loaded.
    ///
    /// Register [`TerrainMaterialPlugin`](crate::render::TerrainMaterialPlugin) for `M` first.
    /// `config` is usually `asset_server.load("terrains/<name>/config.tc.ron")`.
    /// [`TerrainViewConfig::default`](crate::terrain_view::TerrainViewConfig::default) is a fine start.
    fn spawn_terrain(
        &mut self,
        config: Handle<TerrainConfig>,
        view_config: TerrainViewConfig,
        material: M,
        view: Entity,
    );
}

impl<M: Material> SpawnTerrainCommandsExt<M> for Commands<'_, '_> {
    fn spawn_terrain(
        &mut self,
        config: Handle<TerrainConfig>,
        view_config: TerrainViewConfig,
        material: M,
        view: Entity,
    ) {
        self.queue(move |world: &mut World| {
            world
                .resource_mut::<TerrainsToSpawn<M>>()
                .0
                .push(TerrainToSpawn {
                    config,
                    view_config,
                    material,
                    view,
                });
        });
    }
}
