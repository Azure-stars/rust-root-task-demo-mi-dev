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
mod task;
mod utils;

use alloc::vec::Vec;
use alloc_helper::defind_allocator;
use common::{AlignedPage, RootMessageLabel};
use obj_allocator::{alloc_cap, alloc_cap_size, OBJ_ALLOCATOR};
use sel4::{
    cap_type, debug_println, reply, with_ipc_buffer_mut, BootInfo, CNodeCapData, CapRights,
    MessageInfo, UntypedDesc,
};
use sel4_root_task::root_task;
use task::Sel4Task;
use utils::abs_cptr;
use xmas_elf::ElfFile;

/// Default size of the global allocator
const DEFAULT_ALLOCATOR_SIZE: usize = 0x1_0000;
defind_allocator! {
    /// Define a new global allocator
    /// Size is [DEFAULT_ALLOCATOR_SIZE]
    (GLOBAL_ALLOCATOR, DEFAULT_ALLOCATOR_SIZE)
}

/// Empty seat for page frame allocation.
/// FIXME: Support it for multi-threaded task.
static mut PAGE_FRAME_SEAT: AlignedPage = AlignedPage::new();

/// Get the virtual address of the page seat.
pub fn page_seat_vaddr() -> usize {
    unsafe { PAGE_FRAME_SEAT.ptr() as _ }
}

/// The radix bits of the cnode in the task.
const CNODE_RADIX_BITS: usize = 12;
/// The guard bits of the cnode in the task.
const CNODE_GUARD_BITS: usize = 20;

const TASK_FILES: &[&[u8]] = &[include_bytes!("../../../build/kernel-thread.elf")];

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

    // Kernel Use the Largest Untyped Memory Region
    let kernel_untyped = BootInfo::init_cspace_local_cptr::<sel4::cap_type::Untyped>(
        mem_untyped_start + mem_untypes.pop().expect("can't get any memory region").0,
    );

    // Allocate a untyped memory region for root task
    let root_untyped = mem_untypes
        .pop()
        .expect("can't get any untyped for root-task");

    OBJ_ALLOCATOR.lock().init(
        bootinfo.empty(),
        BootInfo::init_cspace_local_cptr::<sel4::cap_type::Untyped>(
            mem_untyped_start + root_untyped.0,
        ),
    );

    let cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let inner_cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let tcb = alloc_cap::<cap_type::TCB>();
    let vspace = alloc_cap::<cap_type::VSpace>();
    let fault_ep = alloc_cap::<cap_type::Endpoint>();
    cnode
        .relative_bits_with_depth(0, CNODE_RADIX_BITS)
        .mutate(
            &abs_cptr(inner_cnode),
            CNodeCapData::skip(CNODE_GUARD_BITS).into_word() as _,
        )
        .unwrap();

    abs_cptr(BootInfo::null())
        .mutate(
            &abs_cptr(cnode),
            CNodeCapData::skip(CNODE_GUARD_BITS).into_word() as _,
        )
        .unwrap();

    abs_cptr(cnode)
        .mint(
            &abs_cptr(BootInfo::null()),
            CapRights::all(),
            CNodeCapData::skip(CNODE_GUARD_BITS).into_word() as _,
        )
        .unwrap();

    BootInfo::init_thread_asid_pool()
        .asid_pool_assign(vspace)
        .unwrap();

    let mut task = Sel4Task::new(tcb, cnode, fault_ep, vspace);

    // Configure Root Task
    task.configure(CNODE_GUARD_BITS)?;

    // Map stack for the task.
    task.map_stack(10);

    // set task priority and max control priority
    task.tcb
        .tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 255, 255)?;

    // Map elf file for the task.
    task.map_elf(ElfFile::new(TASK_FILES[0]).expect("can't map elf file in root_task"));

    // Transfer a untyped memory to kernel_untyped_memory.
    task.abs_cptr(17 as _)
        .copy(&utils::abs_cptr(kernel_untyped), CapRights::all())
        .unwrap();

    // Resume Kernel-Thread Task.
    task.tcb.tcb_resume().unwrap();

    // Waiting for IPC Call.
    loop {
        let (message, _badge) = fault_ep.recv(());
        debug_println!("received message: {:#x?}, badge: {}", message, _badge);

        // let (irq_handler, irq_num) = with_ipc_buffer(|buffer| {
        //     assert_eq!(message.label(), 2 * size_of::<u64>() as u64);
        //     let regs = buffer.msg_regs();
        //     (regs[0], regs[1])
        // });
        if let Some(info) = RootMessageLabel::try_from(&message) {
            match info {
                RootMessageLabel::RegisterIRQ(irq_handler, irq_num) => {
                    BootInfo::irq_control()
                        .irq_control_get(irq_num, &task.abs_cptr(irq_handler))
                        .unwrap();

                    // Reply message
                    with_ipc_buffer_mut(|buffer| {
                        reply(buffer, MessageInfo::new(0, 0, 0, 0));
                    });
                }
            }
        }
    }

    // Stop Root Task.
    // sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    // unreachable!()
}
