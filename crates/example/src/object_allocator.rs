//
// Copyright 2024, Colias Group, LLC
//
// SPDX-License-Identifier: BSD-2-Clause
//

use core::ops::Range;

use sel4::{
    cap_type, BootInfo, ObjectBlueprint, ObjectBlueprintAArch64, ObjectBlueprintArch,
    ObjectBlueprintArm,
};

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

impl_cap_blueprint!(cap_type::Endpoint, ObjectBlueprint::Endpoint);

impl ObjectAllocator {
    pub(crate) fn new(bootinfo: &sel4::BootInfo) -> Self {
        Self {
            empty_slots: bootinfo.empty(),
            ut: find_largest_untyped(bootinfo),
        }
    }

    pub(crate) fn allocate_ep(&mut self) -> sel4::Endpoint {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut
            .untyped_retype(
                &ObjectBlueprint::Endpoint,
                &BootInfo::init_thread_cnode().relative_self(),
                slot_index,
                1,
            )
            .expect("can't allocate endpoint");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::Endpoint>(slot_index)
    }

    pub(crate) fn allocate_tcb(&mut self) -> sel4::TCB {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut
            .untyped_retype(
                &ObjectBlueprint::TCB,
                &BootInfo::init_thread_cnode().relative_self(),
                slot_index,
                1,
            )
            .expect("can't allocate tcb");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::TCB>(slot_index)
    }

    pub(crate) fn allocate_cnode(&mut self, size_bits: usize) -> sel4::CNode {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut
            .untyped_retype(
                &ObjectBlueprint::CNode { size_bits },
                &BootInfo::init_thread_cnode().relative_self(),
                slot_index,
                1,
            )
            .expect("can't allocate tcb");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::CNode>(slot_index)
    }

    /// Allocate a IRQ Handler
    pub(crate) fn allocate_irq_handler(&mut self) -> sel4::IRQHandler {
        let slot_index = self.empty_slots.next().unwrap();
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::IRQHandler>(slot_index)
    }

    /// Allocate a notification
    pub(crate) fn allocate_notification(&mut self) -> sel4::Notification {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut
            .untyped_retype(
                &ObjectBlueprint::Notification,
                &BootInfo::init_thread_cnode().relative_self(),
                slot_index,
                1,
            )
            .expect("can't allocate notification");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::Notification>(slot_index)
    }

    /// Allocate a VSpace
    pub(crate) fn allocate_vspace(&mut self) -> sel4::VSpace {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut
            .untyped_retype(
                &ObjectBlueprintAArch64::VSpace.into(),
                &BootInfo::init_thread_cnode().relative_self(),
                slot_index,
                1,
            )
            .expect("can't allocate notification");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::VSpace>(slot_index)
    }

    /// Allocate a Small Page (12 bits)
    pub(crate) fn allocate_page(&mut self) -> sel4::SmallPage {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut
            .untyped_retype(
                &ObjectBlueprintArm::SmallPage.into(),
                &BootInfo::init_thread_cnode().relative_self(),
                slot_index,
                1,
            )
            .expect("can't allocate notification");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::SmallPage>(slot_index)
    }

    /// Allocate a Page Table
    pub(crate) fn allocate_pt(&mut self) -> sel4::PT {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut
            .untyped_retype(
                &ObjectBlueprintArch::PT.into(),
                &BootInfo::init_thread_cnode().relative_self(),
                slot_index,
                1,
            )
            .expect("can't allocate notification");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::PT>(slot_index)
    }
    // TODO: Allocate cap with Generic definition.
    // pub(crate) fn allocate<C: CapType>(&mut self) -> sel4::LocalCPtr<C> {
    //     let slot_index = self.empty_slots.next().unwrap();
    //     self.ut
    //         .untyped_retype(
    //             &ObjectBlueprint::Arch(ObjectBlueprintArm::SmallPage),
    //             &BootInfo::init_thread_cnode().relative_self(),
    //             slot_index,
    //             1,
    //         )
    //         .expect("can't allocate notification");
    //     sel4::BootInfo::init_cspace_local_cptr::<C>(slot_index)
    // }
}

fn find_largest_untyped(bootinfo: &sel4::BootInfo) -> sel4::Untyped {
    let (ut_ix, _desc) = bootinfo
        .untyped_list()
        .iter()
        .enumerate()
        .filter(|(_i, desc)| !desc.is_device())
        .max_by_key(|(_i, desc)| desc.size_bits())
        .unwrap();

    let idx = bootinfo.untyped().start + ut_ix;
    sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::Untyped>(idx)
}
