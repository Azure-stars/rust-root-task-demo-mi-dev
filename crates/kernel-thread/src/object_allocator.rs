//
// Copyright 2024, Colias Group, LLC
//
// SPDX-License-Identifier: BSD-2-Clause
//

use core::ops::Range;
use sel4::cap::Untyped;
use spin::Mutex;

pub(crate) struct ObjectAllocator {
    empty_slots: Range<usize>,
    ut: Untyped,
}

pub static OBJ_ALLOCATOR: Mutex<ObjectAllocator> = Mutex::new(ObjectAllocator::empty());

impl ObjectAllocator {
    pub(crate) const fn empty() -> Self {
        Self {
            empty_slots: 0..0,
            ut: sel4::cap::Untyped::from_bits(0),
        }
    }

    pub(crate) fn init(&mut self, empty_range: Range<usize>, untyped: Untyped) {
        self.empty_slots = empty_range;
        self.ut = untyped;
    }

    fn allocate(&mut self, blueprint: sel4::ObjectBlueprint) -> sel4::cap::Unspecified {
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

    pub(crate) fn allocate_fixed_sized<T: sel4::CapTypeForObjectOfFixedSize>(
        &mut self,
    ) -> sel4::Cap<T> {
        self.allocate(T::object_blueprint()).cast()
    }

    pub(crate) fn allocate_variable_sized<T: sel4::CapTypeForObjectOfVariableSize>(
        &mut self,
        size_bits: usize,
    ) -> sel4::Cap<T> {
        self.allocate(T::object_blueprint(size_bits)).cast()
    }

    pub(crate) fn allocate_normal_cap<T: sel4::CapType>(&mut self) -> sel4::Cap<T> {
        self.empty_slots
            .by_ref()
            .map(sel4::init_thread::Slot::from_index)
            .next()
            .unwrap()
            .downcast::<T>()
            .cap()
    }
}
