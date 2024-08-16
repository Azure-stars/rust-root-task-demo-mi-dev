//
// Copyright 2024, Colias Group, LLC
//
// SPDX-License-Identifier: BSD-2-Clause
//

use core::ops::Range;

use sel4::BootInfo;

pub(crate) struct ObjectAllocator {
    empty_slots: Range<usize>,
    ut: sel4::Untyped,
}

impl ObjectAllocator {
    pub(crate) fn new(bootinfo: &sel4::BootInfo) -> Self {
        Self {
            empty_slots: bootinfo.empty(),
            ut: find_largest_untyped(bootinfo),
        }
    }

    pub(crate) fn allocate_ep(&mut self) -> sel4::Endpoint {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut.untyped_retype(
            &sel4::ObjectBlueprint::Endpoint,
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        ).expect("can't allocate endpoint");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::Endpoint>(slot_index)
    }

    pub(crate) fn allocate_tcb(&mut self) -> sel4::TCB {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut.untyped_retype(
            &sel4::ObjectBlueprint::TCB,
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        ).expect("can't allocate tcb");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::TCB>(slot_index)
    }

    pub(crate) fn allocate_cnode(&mut self, size_bits: usize) -> sel4::CNode {
        let slot_index = self.empty_slots.next().unwrap();
        self.ut.untyped_retype(
            &sel4::ObjectBlueprint::CNode { size_bits },
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        ).expect("can't allocate tcb");
        sel4::BootInfo::init_cspace_local_cptr::<sel4::cap_type::CNode>(slot_index)
    }
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
