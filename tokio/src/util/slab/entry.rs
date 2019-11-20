use crate::util::slab::Generation;

pub(crate) trait Entry: Default {
    fn generation(&self) -> Generation;

    fn reset(&self, generation: Generation) -> bool;
}
