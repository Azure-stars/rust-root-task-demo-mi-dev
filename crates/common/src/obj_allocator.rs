use core::ops::Range;
use sel4::{cap::Untyped, init_thread};

pub struct ObjectAllocator {
    empty_slots: Range<usize>,
    ut: Untyped,
}

impl ObjectAllocator {
    pub const fn empty() -> Self {
        Self {
            empty_slots: 0..0,
            ut: sel4::cap::Untyped::from_bits(0),
        }
    }

    pub fn init(&mut self, empty_range: Range<usize>, untyped: Untyped) {
        self.empty_slots = empty_range;
        self.ut = untyped;
    }

    pub fn allocate(&mut self, blueprint: sel4::ObjectBlueprint) -> sel4::cap::Unspecified {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut
            .untyped_retype(
                &blueprint,
                &sel4::init_thread::slot::CNODE.cap().relative_self(),
                slot_index,
                1,
            )
            .unwrap();
        sel4::init_thread::Slot::from_index(slot_index).cap()
    }

    pub fn allocate_fixed_sized<T: sel4::CapTypeForObjectOfFixedSize>(&mut self) -> sel4::Cap<T> {
        self.allocate(T::object_blueprint()).cast()
    }

    pub fn allocate_variable_sized<T: sel4::CapTypeForObjectOfVariableSize>(
        &mut self,
        size_bits: usize,
    ) -> sel4::Cap<T> {
        self.allocate(T::object_blueprint(size_bits)).cast()
    }

    pub fn allocate_normal_cap<T: sel4::CapType>(&mut self) -> sel4::Cap<T> {
        self.empty_slots
            .by_ref()
            .map(sel4::init_thread::Slot::from_index)
            .next()
            .unwrap()
            .downcast::<T>()
            .cap()
    }

    pub fn allocate_slot(&mut self) -> usize {
        self.empty_slots.next().unwrap()
    }
}
