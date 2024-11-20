#![no_std]
#![no_main]

extern crate alloc;

mod obj_allocator;
mod task;
mod utils;
use obj_allocator::{ObjectAllocator, OBJ_ALLOCATOR};
use task::*;
use utils::*;

use alloc::vec::Vec;
use common::RootMessageLabel;
use crate_consts::*;
use include_bytes_aligned::include_bytes_aligned;
use sel4::{
    cap_type::{Endpoint, IrqHandler, Untyped},
    init_thread::{self, suspend_self},
    with_ipc_buffer_mut, CPtr, CapRights, MessageInfo, UntypedDesc,
};
use sel4_root_task::{debug_println, root_task, Never};

static TASK_FILES: &[(&str, &[u8])] = &[(
    "kernel-thread",
    include_bytes_aligned!(16, "../../../build/kernel-thread.elf"),
)];

#[root_task(heap_size = PAGE_SIZE * 32)]
fn main(bootinfo: &sel4::BootInfoPtr) -> sel4::Result<Never> {
    // Sort the untyped memory region by size
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

    // Init Global Object Allocator
    OBJ_ALLOCATOR
        .lock()
        .init(bootinfo.empty().range(), root_task_untyped);

    init_thread::slot::TCB.cap().debug_name(b"root");

    let mut tasks = Vec::new();

    // Used for fault and normal IPC ( Reuse )
    let fault_ep = OBJ_ALLOCATOR.lock().allocate_fixed_sized::<Endpoint>();
    // Used for IRQ Registration with slot transfer
    let irq_ep = OBJ_ALLOCATOR.lock().allocate_fixed_sized::<Endpoint>();
    let common_irq_handler = OBJ_ALLOCATOR.lock().allocate_normal_cap::<IrqHandler>();

    for task in TASK_FILES.iter() {
        tasks.push(build_kernel_thread(
            (fault_ep, tasks.len() as _),
            irq_ep,
            task.0,
            task.1,
            unsafe { init_free_page_addr(bootinfo) },
        )?);
    }

    // Transfer a untyped memory to kernel_untyped_memory.
    tasks[0]
        .abs_cptr_with_depth(DEFAULT_CUSTOM_SLOT, CNODE_RADIX_BITS)
        .copy(
            &init_thread::slot::CNODE.cap().relative(kernel_untyped),
            CapRights::all(),
        )
        .unwrap();

    // Start tasks
    run_tasks(&tasks);

    loop {
        debug_println!("[RootTask]: Waiting for message...");
        let (message, badge) = fault_ep.recv(());
        debug_println!(
            "[RootTask]: Received message: {:?}, badge: {:?}",
            message,
            badge
        );
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
                        sel4::reply(buffer, MessageInfo::new(0, 0, 0, 0));
                    });
                }
                RootMessageLabel::RegisterIRQWithCap(irq_num) => {
                    with_ipc_buffer_mut(|buffer| {
                        sel4::reply(buffer, MessageInfo::new(0, 0, 0, 0));
                    });

                    // send irq handler to kernel thread, TODO: use a common irq handler in constant.
                    init_thread::slot::IRQ_CONTROL
                        .cap()
                        .irq_control_get(
                            irq_num,
                            &init_thread::slot::CNODE.cap().relative(common_irq_handler),
                        )
                        .unwrap();
                    // set cap
                    with_ipc_buffer_mut(|buffer| {
                        buffer.caps_or_badges_mut()[0] = common_irq_handler.bits() as _;
                    });

                    // call
                    let info = MessageInfo::new(0, 0, 1, 0);
                    irq_ep.call(info);
                    debug_println!("[RootTask] Sent IRQ to Kernel Thread");
                }
                _ => {
                    debug_println!("[RootTask] Received IRQ");
                }
            }
        }
        suspend_self()
    }
}
