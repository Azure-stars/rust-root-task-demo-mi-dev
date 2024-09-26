//
// Copyright 2023, Colias Group, LLC
//
// SPDX-License-Identifier: BSD-2-Clause
//

#![no_std]
#![no_main]

extern crate alloc;

mod obj_allocator;
mod task;
mod utils;

use alloc::vec::Vec;
use alloc_helper::define_allocator;
use common::{AlignedPage, RootMessageLabel};
use crate_consts::{DEFAULT_CNODE_SLOT_NUMS, DEFAULT_CUSTOM_SLOT};
use include_bytes_aligned::include_bytes_aligned;
use obj_allocator::{alloc_cap, OBJ_ALLOCATOR};
use sel4::{
    cap_type::{self, Endpoint, Untyped},
    init_thread, reply, with_ipc_buffer, with_ipc_buffer_mut, CPtr, CapRights, MessageInfo,
    UntypedDesc,
};
use sel4_root_task::{debug_println, root_task, Never};
use task::{build_kernel_thread, rebuild_cspace, run_tasks};
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

static TASK_FILES: &[(&str, &[u8])] = &[(
    "kernel-thread",
    include_bytes_aligned!(16, "../../../build/kernel-thread.elf"),
)];

#[root_task]
fn main(bootinfo: &sel4::BootInfoPtr) -> sel4::Result<Never> {
    let mem_untyped_start =
        bootinfo.untyped().start() + bootinfo.untyped_list().partition_point(|x| x.is_device());
    let mut mem_untypes: Vec<(usize, &UntypedDesc)> = bootinfo.untyped_list()
        [bootinfo.kernel_untyped_range().start..]
        .iter()
        .enumerate()
        .collect();
    mem_untypes.sort_by(|a, b| a.1.size_bits().cmp(&b.1.size_bits()));

    // debug info
    {
        debug_println!("mem_untyped_start: {:?}", mem_untyped_start);
        debug_println!("untyped list len: {:?}", bootinfo.untyped_list().len());
        debug_println!("untyped len: {:?}", bootinfo.untyped().len());
        debug_println!(
            "untyped range: {:?}->{:?}",
            bootinfo.untyped().start(),
            bootinfo.untyped().end()
        );

        debug_println!("Untyped List: ");
        mem_untypes.iter().rev().for_each(|(index, untyped)| {
            debug_println!(
                "    Untyped({:03}) paddr: {:#x?} size: {:#x}",
                mem_untyped_start + index,
                untyped.paddr(),
                (1usize << untyped.size_bits())
            );
        });
    }

    // Kernel Use the Largest Untyped Memory Region
    let kernel_untyped = CPtr::from_bits(
        (mem_untyped_start
            + mem_untypes
                .pop()
                .expect("No untyped memory for kernel thread")
                .0)
            .try_into()
            .unwrap(),
    )
    .cast::<Untyped>();

    // Allocate a untyped memory region for root task
    let root_task_untyped = CPtr::from_bits(
        (mem_untyped_start
            + mem_untypes
                .pop()
                .expect("No untyped memory for root task")
                .0)
            .try_into()
            .unwrap(),
    )
    .cast::<Untyped>();

    debug_println!("Kernel Untyped: {:?}", kernel_untyped);
    debug_println!("Root Task Untyped: {:?}", root_task_untyped);

    // Init Object Allocator
    OBJ_ALLOCATOR.lock().init(
        bootinfo.empty().start()..(DEFAULT_CNODE_SLOT_NUMS * DEFAULT_CNODE_SLOT_NUMS - 1),
        root_task_untyped,
    );

    init_thread::slot::TCB.cap().debug_name(b"root");
    rebuild_cspace();

    let mut tasks = Vec::new();

    let fault_ep = alloc_cap::<Endpoint>();
    let irq_ep = alloc_cap::<Endpoint>();

    for task in TASK_FILES.iter() {
        tasks.push(build_kernel_thread(
            (fault_ep, tasks.len() as _),
            task.0,
            ElfFile::new(task.1).expect("[root-task] build kernel thread: Invalid ELF file"),
            irq_ep,
        )?);
    }

    // Transfer a untyped memory to kernel_untyped_memory.
    tasks[0]
        .abs_cptr(DEFAULT_CUSTOM_SLOT)
        .copy(&utils::abs_cptr(kernel_untyped), CapRights::all())
        .unwrap();

    // Start tasks
    run_tasks(&tasks);

    // used for irq handler registration
    let common_irq_handler = alloc_cap::<cap_type::IrqHandler>();

    loop {
        debug_println!("Root Task: Waiting for message");
        let (message, badge) = fault_ep.recv(());
        if let Some(info) = RootMessageLabel::try_from(&message) {
            match info {
                RootMessageLabel::RegisterIRQ(irq_handler, irq_num) => {
                    let slot = &tasks[badge as usize]
                        .cnode
                        .relative(CPtr::from_bits(irq_handler));

                    init_thread::slot::IRQ_CONTROL
                        .cap()
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
                    let slot = &init_thread::slot::CNODE.cap().relative(common_irq_handler);
                    init_thread::slot::IRQ_CONTROL
                        .cap()
                        .irq_control_get(irq_num, slot)
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
            debug_println!("[root-task] recv fault {:#x?}", fault)
        }
    }
}
