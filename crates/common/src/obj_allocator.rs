use core::ops::Range;
use sel4::cap::Untyped;

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

    pub fn allocate_normal_cap<T: sel4::CapType>(&mut self) -> sel4::Cap<T> {
        self.empty_slots
            .by_ref()
            .map(sel4::init_thread::Slot::from_index)
            .next()
            .unwrap()
            .downcast::<T>()
            .cap()
    }

    /// Allocate cap with Generic definition and size_bits before rebuilding the cspace
    pub fn allocate_variable_sized_origin<T: sel4::CapTypeForObjectOfVariableSize>(
        &mut self,
        size_bits: usize,
    ) -> sel4::Cap<T> {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut
            .untyped_retype(
                &T::object_blueprint(size_bits),
                &sel4::init_thread::slot::CNODE.cap().relative_self(),
                slot_index,
                1,
            )
            .unwrap();
        sel4::init_thread::Slot::from_index(slot_index).cap()
    }
}

impl ObjectAllocator {
    pub fn allocate_slot(&mut self) -> (usize, usize, usize) {
        let raw_slot_index = self.empty_slots.next().unwrap();
        let slot_index = raw_slot_index & 0xfff;
        let cnode_index = raw_slot_index >> 12;

        if slot_index == 0 {
            self.ut
                .untyped_retype(
                    &sel4::ObjectBlueprint::CNode { size_bits: 12 },
                    &sel4::init_thread::slot::CNODE.cap().relative_self(),
                    cnode_index,
                    1,
                )
                .expect("can't allocate notification");
        }

        (slot_index, cnode_index, raw_slot_index)
    }
    /// Allocate the slot at the new cspace.
    pub fn allocate_and_retype(
        &mut self,
        blueprint: sel4::ObjectBlueprint,
    ) -> sel4::cap::Unspecified {
        // let slot_index = self.empty_slots.next().unwrap();
        // let cnode_index = (slot_index >> 12) as u64;
        let (slot_index, cnode_index, raw_index) = self.allocate_slot();
        self.ut
            .untyped_retype(
                &blueprint,
                &sel4::init_thread::slot::CNODE
                    .cap()
                    .relative_bits_with_depth(cnode_index as u64, 52),
                slot_index,
                1,
            )
            .unwrap();
        sel4::init_thread::Slot::from_index(raw_index).cap()
    }

    /// Allocate and retype the slot at the new cspace
    pub fn allocate_and_retyped_fixed_sized<T: sel4::CapTypeForObjectOfFixedSize>(
        &mut self,
    ) -> sel4::Cap<T> {
        self.allocate_and_retype(T::object_blueprint()).cast()
    }

    /// ALlocate and retype the slot at the new cspace
    pub fn allocate_and_retyped_variable_sized<T: sel4::CapTypeForObjectOfVariableSize>(
        &mut self,
        size_bits: usize,
    ) -> sel4::Cap<T> {
        self.allocate_and_retype(T::object_blueprint(size_bits))
            .cast()
    }
}
