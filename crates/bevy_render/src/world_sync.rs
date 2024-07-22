use std::{marker::PhantomData, ops::DerefMut};

use bevy_app::Plugin;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    bundle::Bundle, component::Component, entity::{Entity, EntityHashMap}, observer::Trigger, query::With, reflect::ReflectComponent, system::{Commands, Query, ResMut, Resource}, world::{Mut, OnAdd, OnRemove, World}
};
use bevy_hierarchy::DespawnRecursiveExt;
use bevy_reflect::Reflect;

/// Marker component that indicate its entity needs to be Synchronized to render world
#[derive(Component, Clone, Debug, Default, Reflect)]
#[reflect[Component]]
pub struct ToRenderWorld;

#[derive(Component, Deref, Clone, Debug)]
pub struct RenderEntity(Entity);
impl RenderEntity {
    pub fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Deref, Clone, Debug)]
pub struct MainEntity(Entity);
impl MainEntity {
    pub fn entity(&self) -> Entity {
        self.0
    }
}
// marker component that its entity needs to be despawned per frame.
#[derive(Component, Clone, Debug, Default, Reflect)]
pub struct RenderFlyEntity;

pub(crate) enum EntityRecord {
    Added(Entity),
    // (main , render)
    Removed(Entity, Entity),
}

// Entity Record in MainWorld pending to Sync
#[derive(Resource, Default, Deref, DerefMut)]
pub(crate) struct PendingSyncEntity {
    records: Vec<EntityRecord>,
}

// resource to hold main world to entity world mapping
#[derive(Resource, Default, Deref, DerefMut)]
pub struct MainToRenderEntityMap {
    map: EntityHashMap<Entity>,
}

pub(crate) fn entity_sync_system(main_world: &mut World, render_world: &mut World) {
    render_world.resource_scope(|render_world, mut mapper: Mut<MainToRenderEntityMap>| {
        let mapper = mapper.deref_mut();
        main_world.resource_scope(|world, mut pending: Mut<PendingSyncEntity>| {
            for record in pending.drain(..) {
                match record {
                    EntityRecord::Added(e) => {
                        if let Some(mut entity) = world.get_entity_mut(e) {
                            match entity.entry::<RenderEntity>() {
                                bevy_ecs::world::Entry::Occupied(_) => {}
                                bevy_ecs::world::Entry::Vacant(entry) => {
                                    let id = render_world.spawn(MainEntity(e)).id();

                                    mapper.insert(e, id);
                                    entry.insert(RenderEntity(id));
                                }
                            };
                        }
                    }
                    EntityRecord::Removed(e1, e2) => {
                        mapper.remove(&e1);
                        render_world
                            .get_entity_mut(e2)
                            .map(|ec| ec.despawn_recursive());
                    }
                }
            }
        });
    });
}

pub(crate) fn despawn_fly_entity(
    mut command: Commands,
    query: Query<Entity, With<RenderFlyEntity>>,
) {
    query.iter().for_each(|e| {
        // TODO : performant delete
        command.entity(e).despawn_recursive();
    })
}
#[derive(Default)]
pub struct WorldSyncPlugin<B: Bundle> {
    _marker: PhantomData<B>,
}

impl<B: Bundle> Plugin for WorldSyncPlugin<B> {
    fn build(&self, app: &mut bevy_app::App) {
        app.init_resource::<PendingSyncEntity>();
        app.observe(
            |trigger: Trigger<OnAdd, B>, mut pending: ResMut<PendingSyncEntity>| {
                pending.push(EntityRecord::Added(trigger.entity()));
            },
        );
        app.observe(
            |trigger: Trigger<OnRemove, B>,
             mut pending: ResMut<PendingSyncEntity>,
             query: Query<&RenderEntity>| {
                if let Ok(e) = query.get(trigger.entity()) {
                    pending.push(EntityRecord::Removed(trigger.entity(), e.entity()));
                };
            },
        );
    }
}