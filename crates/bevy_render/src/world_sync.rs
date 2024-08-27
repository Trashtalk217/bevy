use bevy_app::Plugin;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    component::Component,
    entity::Entity,
    observer::Trigger,
    query::With,
    reflect::ReflectComponent,
    system::{Local, Query, ResMut, Resource, SystemState},
    world::{Mut, OnAdd, OnRemove, World},
};
use bevy_hierarchy::DespawnRecursiveExt;
use bevy_reflect::Reflect;

/// Marker component that indicates that its entity needs to be Synchronized to the render world
///
/// NOTE: This component should persist throughout the entity's entire lifecycle.
/// If this component is removed from its entity, the entity will be despawned.
#[derive(Component, Clone, Debug, Default, Reflect)]
#[reflect[Component]]
pub struct SyncRenderWorld;

#[derive(Component, Deref, Clone, Debug, Copy)]
/// Marker component added on the main world entities that are synced to the Render World in order to keep track of the corresponding render world entity
pub struct RenderEntity(Entity);
impl RenderEntity {
    #[inline]
    pub fn id(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Deref, Clone, Debug)]
/// Marker component added on the render world entities to keep track of the corresponding main world entity
pub struct MainEntity(Entity);
impl MainEntity {
    #[inline]
    pub fn id(&self) -> Entity {
        self.0
    }
}

// marker component that indicates that its entity needs to be despawned at the end of every frame.
#[derive(Component, Clone, Debug, Default, Reflect)]
pub struct RenderFlyEntity;

pub(crate) enum EntityRecord {
    // When an entity is spawned on the main world, notify the render world so that it can spawn a corresponding entity. This contains the main world entity
    Added(Entity),
    // When an entity is despawned on the main world, notify the render world so that the corresponding entity can be despawned. This contains the render world entity.
    Removed(Entity),
}

// Entity Record in MainWorld pending to Sync
#[derive(Resource, Default, Deref, DerefMut)]
pub(crate) struct PendingSyncEntity {
    records: Vec<EntityRecord>,
}

pub(crate) fn entity_sync_system(main_world: &mut World, render_world: &mut World) {
    main_world.resource_scope(|world, mut pending: Mut<PendingSyncEntity>| {
        // TODO : batching record
        for record in pending.drain(..) {
            match record {
                EntityRecord::Added(e) => {
                    if let Some(mut entity) = world.get_entity_mut(e) {
                        match entity.entry::<RenderEntity>() {
                            bevy_ecs::world::Entry::Occupied(_) => {}
                            bevy_ecs::world::Entry::Vacant(entry) => {
                                let id = render_world.spawn(MainEntity(e)).id();

                                entry.insert(RenderEntity(id));
                            }
                        };
                    }
                }
                EntityRecord::Removed(e) => {
                    if let Some(ec) = render_world.get_entity_mut(e) {
                        ec.despawn_recursive();
                    };
                }
            }
        }
    });
}

// TODO: performance remove matched archetype
pub(crate) fn despawn_fly_entity(
    world: &mut World,
    state: &mut SystemState<Query<Entity, With<RenderFlyEntity>>>,
    mut local: Local<Vec<Entity>>,
) {
    let query = state.get(world);

    local.extend(query.iter());

    // ensure next frame allocation keeps order
    local.sort_unstable_by_key(|e| e.index());
    for e in local.drain(..).rev() {
        world.despawn(e);
    }
}

/// A Plugin that synchronizes entities with specific Components between the main world and render world.
#[derive(Default)]
pub struct WorldSyncPlugin;

impl Plugin for WorldSyncPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.init_resource::<PendingSyncEntity>();
        app.observe(
            |trigger: Trigger<OnAdd, SyncRenderWorld>, mut pending: ResMut<PendingSyncEntity>| {
                pending.push(EntityRecord::Added(trigger.entity()));
            },
        );
        app.observe(
            |trigger: Trigger<OnRemove, SyncRenderWorld>,
             mut pending: ResMut<PendingSyncEntity>,
             query: Query<&RenderEntity>| {
                if let Ok(e) = query.get(trigger.entity()) {
                    pending.push(EntityRecord::Removed(e.id()));
                };
            },
        );
    }
}
