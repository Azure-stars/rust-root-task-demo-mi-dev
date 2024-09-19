//
// Copyright 2023, Colias Group, LLC
//
// SPDX-License-Identifier: BSD-2-Clause
//

#![no_std]
#![no_main]
#![feature(never_type)]
#![feature(const_trait_impl)]
#![feature(effects)]

extern crate alloc;

mod obj_allocator;
use alloc::vec::Vec;
use alloc_helper::define_allocator;
use obj_allocator::OBJ_ALLOCATOR;
use sel4::{debug_println, BootInfo, UntypedDesc};
use sel4_root_task::root_task;

/// Default size of the global allocator
const DEFAULT_ALLOCATOR_SIZE: usize = 0x1_0000;
define_allocator! {
    /// Define a new global allocator
    /// Size is [DEFAULT_ALLOCATOR_SIZE]
    (GLOBAL_ALLOCATOR, DEFAULT_ALLOCATOR_SIZE)
}

#[root_task]
fn main(bootinfo: &sel4::BootInfo) -> sel4::Result<!> {
    let mem_untyped_start =
        bootinfo.untyped().start + bootinfo.untyped_list().partition_point(|x| x.is_device());
    let mut mem_untypes: Vec<(usize, &UntypedDesc)> =
        bootinfo.kernel_untyped_list().iter().enumerate().collect();
    // Sort the Untyped Caps by size in incresing order.
    mem_untypes.sort_by(|a, b| a.1.size_bits().partial_cmp(&b.1.size_bits()).unwrap());
    // Display Untyped Caps information
    debug_println!("Untyped List: ");
    mem_untypes.iter().rev().for_each(|(index, untyped)| {
        debug_println!(
            "    Untyped({:03}) paddr: {:#x?} size: {:#x}",
            mem_untyped_start + index,
            untyped.paddr(),
            (1usize << untyped.size_bits())
        );
    });

    let root_untyped = mem_untypes.pop().unwrap();

    OBJ_ALLOCATOR.lock().init(
        bootinfo.empty().start..(4096 * 4096),
        BootInfo::init_cspace_local_cptr::<sel4::cap_type::Untyped>(
            mem_untyped_start + root_untyped.0,
        ),
    );

    BootInfo::init_thread_tcb().debug_name(b"root");

    debug_println!("Root Task: Say Goodbye!");

    sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    unreachable!()
}
