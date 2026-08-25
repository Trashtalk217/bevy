use core::ptr::NonNull;

use bevy_ptr::{move_as_ptr, ConstNonNull};

use crate::{
    archetype::{Archetype, ArchetypeCreated, ArchetypeId, SpawnBundleStatus},
    bundle::{Bundle, BundleInfo, NoBundleEffect},
    change_detection::MaybeLocation,
    change_detection::Tick,
    entity::{AllocEntitiesIterator, Entity, EntitySetIterator},
    storage::Table,
    world::{InsertMode, UnsafeWorldCell, World},
};
use core::iter::FusedIterator;

/// An iterator that spawns a series of entities and returns the [ID](Entity) of
/// each spawned entity.
///
/// If this iterator is not fully exhausted, any remaining entities will be spawned when this type is dropped.
pub struct SpawnBatchIter<'w, I>
where
    I: Iterator,
    I::Item: Bundle<Effect: NoBundleEffect>,
{
    inner: I,
    world: UnsafeWorldCell<'w>,
    bundle_info: ConstNonNull<BundleInfo>,
    table: NonNull<Table>,
    archetype: NonNull<Archetype>,
    change_tick: Tick,
    allocator: AllocEntitiesIterator<'w>,
    caller: MaybeLocation,
}

impl<'w, I> SpawnBatchIter<'w, I>
where
    I: Iterator,
    I::Item: Bundle<Effect: NoBundleEffect>,
{
    #[inline]
    #[track_caller]
    pub(crate) fn new(world: &'w mut World, iter: I, caller: MaybeLocation) -> Self {
        let change_tick = world.change_tick();

        let (lower, upper) = iter.size_hint();
        let length = upper.unwrap_or(lower);

        let bundle_id = world.register_bundle_info::<I::Item>();
        // SAFETY: bundle exists per precondition
        let bundle_info = unsafe { world.bundles.get_unchecked(bundle_id) };
        // SAFETY: retrieved from same world in previous line
        let (new_archetype_id, is_new_created) = unsafe {
            world.archetypes.insert_bundle_into_archetype(
                bundle_info,
                &mut world.storages,
                &world.components,
                &world.observers,
                ArchetypeId::EMPTY,
            )
        };

        let archetype = &mut world.archetypes[new_archetype_id];
        let table = &mut world.storages.tables[archetype.table_id()];

        // reserve storage
        archetype.reserve(length);
        table.reserve(length);

        let archetype: NonNull<Archetype> = archetype.into();
        let table: NonNull<Table> = table.into();

        // DEFINITELY WRONG
        if is_new_created {
            // SAFETY:
            // - we have exclusive ownership of the world and hold no references to command queue or component data
            // - as this goes through `DeferredWorld`, our pointers will not be invalidated
            unsafe { world.as_unsafe_world_cell_readonly().into_deferred() }
                .trigger(ArchetypeCreated(new_archetype_id));
        }

        Self {
            inner: iter,
            allocator: world.entity_allocator.alloc_many(length as u32),
            world: world.as_unsafe_world_cell_readonly(), // HIGHLY ILLEGAL
            bundle_info: bundle_info.into(),
            table: table.into(),
            archetype,
            change_tick,
            caller,
        }
    }
}

impl<I> Drop for SpawnBatchIter<'_, I>
where
    I: Iterator,
    I::Item: Bundle<Effect: NoBundleEffect>,
{
    fn drop(&mut self) {
        // Iterate through self in order to spawn remaining bundles.
        for _ in &mut *self {}
        // Free all the over allocated entities.
        for e in self.allocator.by_ref() {
            // SAFETY: TODO
            unsafe { self.world.world_mut() }.entity_allocator.free(e);
        }
        // Apply any commands from those operations.
        // SAFETY: `self.spawner` will be dropped immediately after this call.
        unsafe { self.world.world_mut().flush() };
    }
}

impl<I> Iterator for SpawnBatchIter<'_, I>
where
    I: Iterator,
    I::Item: Bundle<Effect: NoBundleEffect>,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        let bundle = self.inner.next()?;
        move_as_ptr!(bundle);
        let entity = self.allocator.next().unwrap_or_else(|| {
            // Safety: TODO
            unsafe { &mut self.world.world_mut().entity_allocator }.alloc()
        });

        // SAFETY: We do not make any structural changes to the archetype graph through self.world so these pointers always remain valid
        let bundle_info = unsafe { self.bundle_info.as_ref() };
        // SAFETY: exclusive world access; reference does not outlife this block
        let table = unsafe { self.table.as_mut() };
        // SAFETY: exclusive world access; reference does not outlife this block
        let archetype = unsafe { self.archetype.as_mut() };

        // SAFETY:
        // - bundle matches spawner type and we just allocated it
        // - I::Item::Effect: NoBundleEffect
        let (sparse_sets, entities) = {
            // SAFETY: has read+write perms, not used to access tables or archetypes, will be dropped after this block
            let world = unsafe { self.world.world_mut() };
            (&mut world.storages.sparse_sets, &mut world.entities)
        };
        // SAFETY: Component data will be written
        let table_row = unsafe { table.allocate(entity) };
        // SAFETY: row was just allocated & component data will be written
        let location = unsafe { archetype.allocate(entity, table_row) };
        // SAFETY:
        // - bundle component status is always added, as entity previously didn't exist per precondition
        // - `apply_effect` called if needed per precondition
        // - table_row was just allocated, bundle matches
        unsafe {
            bundle_info.write_components(
                table,
                sparse_sets,
                &SpawnBundleStatus,
                bundle_info.required_component_constructors.iter(),
                entity,
                table_row,
                self.change_tick,
                bundle,
                InsertMode::Replace,
                self.caller,
            );
        }
        // SAFETY: Entity was just spawned at this location
        unsafe {
            entities.set_location(entity.index(), Some(location));
            entities.mark_spawned_or_despawned(entity.index(), self.caller, self.change_tick);
        };
        Some(entity)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I, T> ExactSizeIterator for SpawnBatchIter<'_, I>
where
    I: ExactSizeIterator<Item = T>,
    T: Bundle<Effect: NoBundleEffect>,
{
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<I, T> FusedIterator for SpawnBatchIter<'_, I>
where
    I: FusedIterator<Item = T>,
    T: Bundle<Effect: NoBundleEffect>,
{
}

// SAFETY: Newly spawned entities are unique.
unsafe impl<I: Iterator, T> EntitySetIterator for SpawnBatchIter<'_, I>
where
    I: FusedIterator<Item = T>,
    T: Bundle<Effect: NoBundleEffect>,
{
}

#[cfg(test)]
mod tests {
    use bevy_ecs_macros::Component;

    use super::*;

    #[derive(Clone, Copy, Component)]
    struct ComponentA;

    #[test]
    fn spawn_batch_does_not_leak_entities() {
        let mut world = World::new();
        world.spawn_batch((0u32..50).filter(|&i| i & 1 > 0).map(|_| ComponentA));
        let total_allocated = world.entity_allocator().inner.total_entity_indices();
        world.entity_allocator_mut().inner.flush_freed();
        world.entity_allocator_mut().alloc();
        let reused = world.entity_allocator().inner.total_entity_indices() == total_allocated;
        assert!(reused);
    }
}
