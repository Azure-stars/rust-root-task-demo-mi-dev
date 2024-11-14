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
use common::AlignedPage;
use include_bytes_aligned::include_bytes_aligned;
use obj_allocator::{ObjectAllocator, OBJ_ALLOCATOR};
use object::Object;
use sel4::{
    cap_type::{Endpoint, Untyped},
    init_thread, CPtr, UntypedDesc,
};
use sel4_root_task::{debug_println, root_task, Never};
use task::*;
use utils::*;
use xmas_elf::ElfFile;

/// Default size of the global allocator
const DEFAULT_ALLOCATOR_SIZE: usize = 0x1_0000;
define_allocator! {
    /// Define a new global allocator
    /// Size is [DEFAULT_ALLOCATOR_SIZE]
    (GLOBAL_ALLOCATOR, DEFAULT_ALLOCATOR_SIZE)
}

/// The radix bits of the cnode in the task.
const CNODE_RADIX_BITS: usize = 12;

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
    OBJ_ALLOCATOR
        .lock()
        .init(bootinfo.empty().range(), root_task_untyped);

    init_thread::slot::TCB.cap().debug_name(b"root");

    let child_image = object::File::parse(TASK_FILES[0].1).unwrap();

    // make 新线程的虚拟地址空间
    let (child_vspace, ipc_buffer_addr, ipc_buffer_cap) = make_child_vspace(
        &child_image,
        sel4::init_thread::slot::VSPACE.cap(),
        unsafe { init_free_page_addr(bootinfo) },
        sel4::init_thread::slot::ASID_POOL.cap(),
    );

    let child_cnode_size_bits = CNODE_RADIX_BITS;
    // make 新线程的 CNode
    let child_cnode = OBJ_ALLOCATOR
        .lock()
        .allocate_variable_sized::<sel4::cap_type::CNode>(child_cnode_size_bits);

    let inter_task_nfn = OBJ_ALLOCATOR
        .lock()
        .allocate_fixed_sized::<sel4::cap_type::Notification>();

    child_cnode
        .relative_bits_with_depth(1, child_cnode_size_bits)
        .mint(
            &sel4::init_thread::slot::CNODE
                .cap()
                .relative(inter_task_nfn),
            sel4::CapRights::write_only(),
            0,
        )
        .unwrap();

    let child_tcb = OBJ_ALLOCATOR
        .lock()
        .allocate_fixed_sized::<sel4::cap_type::Tcb>();

    child_tcb
        .tcb_configure(
            sel4::init_thread::slot::NULL.cptr(),
            child_cnode,
            sel4::CNodeCapData::new(0, sel4::WORD_SIZE - child_cnode_size_bits),
            child_vspace,
            ipc_buffer_addr as sel4::Word,
            ipc_buffer_cap,
        )
        .unwrap();

    child_cnode
        .relative_bits_with_depth(2, child_cnode_size_bits)
        .mint(
            &sel4::init_thread::slot::CNODE.cap().relative(child_tcb),
            sel4::CapRights::all(),
            0,
        )
        .unwrap();

    let mut ctx = sel4::UserContext::default();
    *ctx.pc_mut() = child_image.entry().try_into().unwrap();
    child_tcb.tcb_write_all_registers(true, &mut ctx).unwrap();

    inter_task_nfn.wait();

    // rebuild_cspace();

    // let mut tasks = Vec::new();

    // let fault_ep = OBJ_ALLOCATOR.lock().allocate_fixed_sized::<Endpoint>();
    // let irq_ep = OBJ_ALLOCATOR.lock().allocate_fixed_sized::<Endpoint>();

    // for task in TASK_FILES.iter() {
    //     tasks.push(build_kernel_thread(
    //         (fault_ep, tasks.len() as _),
    //         task.0,
    //         task.1,
    //         irq_ep,
    //     )?);
    // }

    // // Transfer a untyped memory to kernel_untyped_memory.
    // tasks[0]
    //     .abs_cptr(DEFAULT_CUSTOM_SLOT)
    //     .copy(&utils::abs_cptr(kernel_untyped), CapRights::all())
    //     .unwrap();

    // // Start tasks
    // run_tasks(&tasks);

    // used for irq handler registration
    // let common_irq_handler = OBJ_ALLOCATOR
    //     .lock()
    //     .allocate_normal_cap::<sel4::cap_type::IrqHandler>();

    sel4::debug_println!("TEST_PASS");

    sel4::init_thread::suspend_self()

    // loop {
    //     debug_println!("Root Task: Waiting for message");
    //     let (message, badge) = fault_ep.recv(());
    //     if let Some(info) = RootMessageLabel::try_from(&message) {
    //         match info {
    //             RootMessageLabel::RegisterIRQ(irq_handler, irq_num) => {
    //                 let slot = &tasks[badge as usize]
    //                     .cnode
    //                     .relative(CPtr::from_bits(irq_handler));

    //                 init_thread::slot::IRQ_CONTROL
    //                     .cap()
    //                     .irq_control_get(irq_num, slot)
    //                     .unwrap();

    //                 with_ipc_buffer_mut(|buffer| {
    //                     reply(buffer, MessageInfo::new(0, 0, 0, 0));
    //                 });
    //             }
    //             RootMessageLabel::TranslateAddr(addr) => {
    //                 let phys_addr = tasks[badge as usize]
    //                     .mapped_page
    //                     .get(&(addr / 0x1000 * 0x1000));
    //                 let message = RootMessageLabel::TranslateAddr(
    //                     phys_addr.unwrap().frame_get_address().unwrap() + addr % 0x1000,
    //                 )
    //                 .build();
    //                 with_ipc_buffer_mut(|buffer| reply(buffer, message));
    //             }
    //             RootMessageLabel::RegisterIRQWithCap(irq_num) => {
    //                 with_ipc_buffer_mut(|buffer| {
    //                     reply(buffer, MessageInfo::new(0, 0, 0, 0));
    //                 });

    //                 // send irq handler to kernel thread, TODO: use a common irq handler in constant.
    //                 let slot = &init_thread::slot::CNODE.cap().relative(common_irq_handler);
    //                 init_thread::slot::IRQ_CONTROL
    //                     .cap()
    //                     .irq_control_get(irq_num, slot)
    //                     .unwrap();

    //                 // set cap
    //                 with_ipc_buffer_mut(|buffer| {
    //                     buffer.caps_or_badges_mut()[0] = common_irq_handler.bits() as _;
    //                 });

    //                 // call
    //                 let info = MessageInfo::new(0, 0, 1, 0);
    //                 irq_ep.call(info);
    //             }
    //         }
    //     } else {
    //         let fault = with_ipc_buffer(|buffer| sel4::Fault::new(buffer, &message));
    //         debug_println!("[root-task] recv fault {:#x?}", fault)
    //     }
    // }
}
