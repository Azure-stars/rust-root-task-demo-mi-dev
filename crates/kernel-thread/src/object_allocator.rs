//
// Copyright 2024, Colias Group, LLC
//
// SPDX-License-Identifier: BSD-2-Clause
//

use core::ops::Range;

use sel4::{
    cap_type, BootInfo, ObjectBlueprint, ObjectBlueprintAArch64, ObjectBlueprintArch,
    ObjectBlueprintArm, Untyped,
};
use spin::Mutex;

pub(crate) struct ObjectAllocator {
    empty_slots: Range<usize>,
    ut: sel4::Untyped,
}

trait AllocateCapBluePrint {
    fn get_blueprint() -> ObjectBlueprint;
}

macro_rules! impl_cap_blueprint {
    ($cap:ty, $bp: expr) => {
        impl AllocateCapBluePrint for $cap {
            fn get_blueprint() -> ObjectBlueprint {
                $bp
            }
        }
    };
}

pub static OBJ_ALLOCATOR: Mutex<ObjectAllocator> = Mutex::new(ObjectAllocator::empty());

impl_cap_blueprint!(cap_type::Endpoint, ObjectBlueprint::Endpoint);

impl ObjectAllocator {
    /// Create a new empty ObjectAllocator
    pub(crate) const fn empty() -> Self {
        Self {
            empty_slots: 0..0,
            ut: sel4::Untyped::from_bits(0),
        }
    }

    /// Init object allocator with [sel4::BootInfo]
    pub(crate) fn init(&mut self, empty_range: Range<usize>, untyped: Untyped) {
        self.empty_slots = empty_range;
        self.ut = untyped;
    }
}

/// Allocate a [sel4::Endpoint]
pub(crate) fn allocate_ep() -> sel4::Endpoint {
    let mut allocator = OBJ_ALLOCATOR.lock();

    let slot_index = allocator.empty_slots.next().unwrap();
    allocator
        .ut
        .untyped_retype(
            &ObjectBlueprint::Endpoint,
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        )
        .expect("can't allocate endpoint");
    sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::Endpoint>(slot_index)
}

/// Allocate a [sel4::TCB]
pub(crate) fn allocate_tcb() -> sel4::TCB {
    let mut allocator = OBJ_ALLOCATOR.lock();

    let slot_index = allocator.empty_slots.next().unwrap();
    allocator
        .ut
        .untyped_retype(
            &ObjectBlueprint::TCB,
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        )
        .expect("can't allocate tcb");
    sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::TCB>(slot_index)
}

/// Allocate a [sel4::CNode]
pub(crate) fn allocate_cnode(size_bits: usize) -> sel4::CNode {
    let mut allocator = OBJ_ALLOCATOR.lock();

    let slot_index = allocator.empty_slots.next().unwrap();
    allocator
        .ut
        .untyped_retype(
            &ObjectBlueprint::CNode { size_bits },
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        )
        .expect("can't allocate tcb");
    sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::CNode>(slot_index)
}

/// Allocate a [sel4::IRQHandler]
pub(crate) fn allocate_irq_handler() -> sel4::IRQHandler {
    let slot_index = OBJ_ALLOCATOR.lock().empty_slots.next().unwrap();
    sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::IRQHandler>(slot_index)
}

/// Allocate a [sel4::Notification]
pub(crate) fn allocate_notification() -> sel4::Notification {
    let mut allocator = OBJ_ALLOCATOR.lock();

    let slot_index = allocator.empty_slots.next().unwrap();
    allocator
        .ut
        .untyped_retype(
            &ObjectBlueprint::Notification,
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        )
        .expect("can't allocate notification");
    sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::Notification>(slot_index)
}

/// Allocate a [sel4::VSpace]
pub(crate) fn allocate_vspace() -> sel4::VSpace {
    let mut allocator = OBJ_ALLOCATOR.lock();

    let slot_index = allocator.empty_slots.next().unwrap();
    allocator
        .ut
        .untyped_retype(
            &ObjectBlueprintAArch64::VSpace.into(),
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        )
        .expect("can't allocate notification");
    sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::VSpace>(slot_index)
}

/// Allocate a [sel4::SmallPage] (12 bits)
pub(crate) fn allocate_page() -> sel4::SmallPage {
    let mut allocator = OBJ_ALLOCATOR.lock();

    let slot_index = allocator.empty_slots.next().unwrap();
    allocator
        .ut
        .untyped_retype(
            &ObjectBlueprintArm::SmallPage.into(),
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        )
        .expect("can't allocate notification");
    sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::SmallPage>(slot_index)
}

/// Allocate a [sel4::PT]
pub(crate) fn allocate_pt() -> sel4::PT {
    let mut allocator = OBJ_ALLOCATOR.lock();
    let slot_index = allocator.empty_slots.next().unwrap();
    allocator
        .ut
        .untyped_retype(
            &ObjectBlueprintArch::PT.into(),
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        )
        .expect("can't allocate notification");
    sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::PT>(slot_index)
}
