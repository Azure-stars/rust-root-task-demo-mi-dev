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
mod tests;
mod utils;

use alloc::vec::Vec;
use alloc_helper::define_allocator;
use common::{AlignedPage, RootMessageLabel, VIRTIO_MMIO_ADDR};
use crate_consts::{DEFAULT_CNODE_SLOT_NUMS, DEFAULT_CUSTOM_SLOT};
use include_bytes_aligned::include_bytes_aligned;
use obj_allocator::{alloc_cap, alloc_cap_size, alloc_cap_size_slot, OBJ_ALLOCATOR};
use sel4::{
    cap_type, debug_println, reply, with_ipc_buffer, with_ipc_buffer_mut, BootInfo, CNode,
    CNodeCapData, CPtr, CapRights, Endpoint, LargePage, MessageInfo, ObjectBlueprintArm,
    UntypedDesc, VMAttributes,
};
use sel4_root_task::root_task;
use task::Sel4Task;
use utils::{abs_cptr, sys_null};
use xmas_elf::ElfFile;

/// Default size of the global allocator
const DEFAULT_ALLOCATOR_SIZE: usize = 0x1_0000;
define_allocator! {
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

static TASK_FILES: &[(&str, &[u8])] = &[
    (
        "kernel-thread",
        include_bytes_aligned!(16, "../../../build/kernel-thread.elf"),
    ),
    (
        "block-thread",
        include_bytes_aligned!(16, "../../../build/blk-thread.elf"),
    ),
    (
        "net-thread",
        include_bytes_aligned!(16, "../../../build/net-thread.elf"),
    ),
];

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
        bootinfo.empty().start..(DEFAULT_CNODE_SLOT_NUMS * DEFAULT_CNODE_SLOT_NUMS),
        BootInfo::init_cspace_local_cptr::<sel4::cap_type::Untyped>(
            mem_untyped_start + root_untyped.0,
        ),
    );

    BootInfo::init_thread_tcb().debug_name(b"root");

    rebuild_cspaces();

    let mut tasks = Vec::new();

    let fault_ep = alloc_cap::<cap_type::Endpoint>();
    let irq_ep = alloc_cap::<cap_type::Endpoint>();

    // Channel to send message to blk thread
    let blk_dev_ep = alloc_cap::<cap_type::Endpoint>();
    let net_dev_ep = alloc_cap::<cap_type::Endpoint>();
    // Create kernel-thread and block-thread tasks.
    for file in TASK_FILES.iter() {
        tasks.push(build_kernel_thread(
            (fault_ep, tasks.len() as _),
            file.0,
            ElfFile::new(file.1).expect("can't map elf file in root_task"),
            irq_ep,
        )?);
    }

    tests::test_entry();

    // Transfer a untyped memory to kernel_untyped_memory.
    tasks[0]
        .abs_cptr(DEFAULT_CUSTOM_SLOT)
        .copy(&utils::abs_cptr(kernel_untyped), CapRights::all())
        .unwrap();

    tasks[0]
        .abs_cptr(DEFAULT_CUSTOM_SLOT + 1)
        .copy(&utils::abs_cptr(blk_dev_ep), CapRights::all())
        .unwrap();

    tasks[0]
        .abs_cptr(DEFAULT_CUSTOM_SLOT + 2)
        .copy(&utils::abs_cptr(net_dev_ep), CapRights::all())
        .unwrap();

    // Set Notification for Blk-Thread Task.
    let net_irq_not = alloc_cap::<cap_type::Notification>();
    tasks[1]
        .abs_cptr(DEFAULT_CUSTOM_SLOT)
        .copy(&utils::abs_cptr(net_irq_not), CapRights::all())
        .unwrap();
    tasks[1]
        .abs_cptr(DEFAULT_CUSTOM_SLOT + 2)
        .copy(&utils::abs_cptr(blk_dev_ep), CapRights::all())
        .unwrap();

    // Map device memory to blk-thread task
    let finded_device_idx = bootinfo
        .device_untyped_list()
        .iter()
        .position(|x| {
            x.paddr() < VIRTIO_MMIO_ADDR && x.paddr() + (1 << x.size_bits()) > VIRTIO_MMIO_ADDR
        })
        .expect("can't find device memory");
    let device_untyped = BootInfo::init_cspace_local_cptr::<sel4::cap_type::Untyped>(
        bootinfo.untyped().start + finded_device_idx,
    );

    let device_frame = {
        let slot_pos = OBJ_ALLOCATOR.lock().alloc_slot();
        device_untyped
            .untyped_retype(
                &ObjectBlueprintArm::LargePage.into(),
                &BootInfo::init_thread_cnode().relative_bits_with_depth(slot_pos.1 as _, 52),
                slot_pos.0,
                1,
            )
            .unwrap();
        sel4::BootInfo::init_cspace_local_cptr::<cap_type::LargePage>(slot_pos.2)
    };

    // FIXME: assert device frame area.
    assert!(device_frame.frame_get_address().unwrap() < VIRTIO_MMIO_ADDR);
    device_frame
        .frame_map(
            tasks[1].vspace,
            0x1_2000_0000,
            CapRights::all(),
            VMAttributes::DEFAULT,
        )
        .unwrap();

    // Map DMA frame.
    tasks[1].map_page(0x1_0000_3000, alloc_cap::<cap_type::Granule>());
    tasks[1].map_page(0x1_0000_4000, alloc_cap::<cap_type::Granule>());

    // Resumt Block Thread Task.
    let device_slot = OBJ_ALLOCATOR.lock().alloc_slot();
    abs_cptr(LargePage::from_bits(device_slot.2 as _))
        .copy(&abs_cptr(device_frame), CapRights::all())
        .unwrap();
    LargePage::from_bits(device_slot.2 as _)
        .frame_map(
            tasks[2].vspace,
            0x1_2000_0000,
            CapRights::all(),
            VMAttributes::DEFAULT,
        )
        .unwrap();

    tasks[2]
        .abs_cptr(DEFAULT_CUSTOM_SLOT)
        .copy(
            &utils::abs_cptr(alloc_cap::<cap_type::Notification>()),
            CapRights::all(),
        )
        .unwrap();

    tasks[2]
        .abs_cptr(DEFAULT_CUSTOM_SLOT + 2)
        .copy(&utils::abs_cptr(net_dev_ep), CapRights::all())
        .unwrap();

    sys_null(-10);

    // Map DMA frame.
    for i in 0..32 {
        tasks[2].map_page(0x1_0000_3000 + i * 0x1000, alloc_cap::<cap_type::Granule>());
    }

    // used for irq handler registration
    let common_irq_handler = alloc_cap::<cap_type::IRQHandler>();

    // Start tasks
    tasks.iter().for_each(Sel4Task::run);

    // Waiting for IPC Call.
    loop {
        let (message, badge) = fault_ep.recv(());
        if let Some(info) = RootMessageLabel::try_from(&message) {
            match info {
                RootMessageLabel::RegisterIRQ(irq_handler, irq_num) => {
                    let slot = &tasks[badge as usize]
                        .cnode
                        .relative(CPtr::from_bits(irq_handler));

                    BootInfo::irq_control()
                        .irq_control_get(irq_num, slot)
                        .unwrap();

                    with_ipc_buffer_mut(|buffer| {
                        reply(buffer, MessageInfo::new(0, 0, 0, 0));
                    });
                }
                RootMessageLabel::TranslateAddr(addr) => {
                    let phys_addr = tasks[badge as usize]
                        .mapped_page
                        .get(&(addr / 0x1000 * 0x1000));
                    let message = RootMessageLabel::TranslateAddr(
                        phys_addr.unwrap().frame_get_address().unwrap() + addr % 0x1000,
                    )
                    .build();
                    with_ipc_buffer_mut(|buffer| reply(buffer, message));
                }
                RootMessageLabel::RegisterIRQWithCap(irq_num) => {
                    with_ipc_buffer_mut(|buffer| {
                        reply(buffer, MessageInfo::new(0, 0, 0, 0));
                    });

                    // send irq handler to kernel thread, TODO: use a common irq handler in constant.
                    BootInfo::irq_control()
                        .irq_control_get(
                            irq_num,
                            &BootInfo::init_thread_cnode().relative(common_irq_handler),
                        )
                        .unwrap();
                    // set cap
                    with_ipc_buffer_mut(|buffer| {
                        buffer.caps_or_badges_mut()[0] = common_irq_handler.bits() as _;
                    });

                    // call
                    let info = MessageInfo::new(0, 0, 1, 0);
                    irq_ep.call(info);
                }
            }
        } else {
            let fault = with_ipc_buffer(|buffer| sel4::Fault::new(buffer, &message));
            debug_println!("fault {:#x?}", fault)
        }
    }

    // Stop Root Task.
    // sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    // unreachable!()
}

fn build_kernel_thread(
    fault_ep: (Endpoint, u64),
    thread_name: &str,
    elf_file: ElfFile,
    irq_ep: Endpoint,
) -> sel4::Result<Sel4Task> {
    let cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let inner_cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let tcb = alloc_cap::<cap_type::TCB>();
    let vspace = alloc_cap::<cap_type::VSpace>();

    // Build 2 level CSpace.
    // | unused (40 bits) | Level1 (12 bits) | Level0 (12 bits) |
    cnode
        .relative_bits_with_depth(0, CNODE_RADIX_BITS)
        .mutate(
            &abs_cptr(inner_cnode),
            CNodeCapData::skip(0).into_word() as _,
        )
        .unwrap();
    abs_cptr(BootInfo::null())
        .mutate(
            &abs_cptr(cnode),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word() as _,
        )
        .unwrap();
    abs_cptr(cnode)
        .mutate(
            &abs_cptr(BootInfo::null()),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word() as _,
        )
        .unwrap();

    BootInfo::init_thread_asid_pool()
        .asid_pool_assign(vspace)
        .unwrap();

    let mut task = Sel4Task::new(tcb, cnode, fault_ep.0, vspace, fault_ep.1, irq_ep);

    // Configure Root Task
    task.configure(2 * CNODE_RADIX_BITS)?;

    // Map stack for the task.
    task.map_stack(10);

    // set task priority and max control priority
    task.tcb
        .tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 255, 255)?;

    // Map elf file for the task.
    task.map_elf(elf_file);

    task.tcb.debug_name(thread_name.as_bytes());

    debug_println!("Task: {} created. cnode: {:?}", thread_name, task.cnode);

    Ok(task)
}

/// The default cspace is 12 bits, has 1024 slots. But it is not enough,
/// rebuild to 2 level 24 bits in the here.
fn rebuild_cspaces() {
    let cnode = alloc_cap_size_slot::<cap_type::CNode>(CNODE_RADIX_BITS);

    cnode
        .relative_bits_with_depth(0, CNODE_RADIX_BITS)
        .mint(
            &BootInfo::init_thread_cnode().relative(BootInfo::init_thread_cnode()),
            CapRights::all(),
            CNodeCapData::skip(0).into_word(),
        )
        .unwrap();

    // Load
    BootInfo::init_thread_cnode()
        .relative(BootInfo::null())
        .mutate(
            &BootInfo::init_thread_cnode().relative(BootInfo::init_thread_cnode()),
            CNodeCapData::skip_high_bits(CNODE_RADIX_BITS).into_word(),
        )
        .unwrap();

    CNode::from_bits(0)
        .relative(BootInfo::init_thread_cnode())
        .mint(
            &CNode::from_bits(0).relative(cnode),
            CapRights::all(),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word(),
        )
        .unwrap();

    BootInfo::init_thread_cnode()
        .relative(BootInfo::null())
        .delete()
        .unwrap();

    BootInfo::init_thread_tcb().invoke(|cptr, buffer| {
        buffer.inner_mut().seL4_TCB_SetSpace(
            cptr.bits(),
            BootInfo::null().cptr().bits(),
            cnode.bits(),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word(),
            BootInfo::init_thread_vspace().bits(),
            0,
        )
    });
}
