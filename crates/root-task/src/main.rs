#![no_std]
#![no_main]
#![feature(never_type)]

extern crate alloc;

mod task;
mod thread;
mod utils;
use task::*;
use task_helper::TaskHelperTrait;
use thread::test_threads;
use utils::*;

use alloc::vec::Vec;
use common::*;
use crate_consts::*;
use include_bytes_aligned::include_bytes_aligned;
use sel4::{
    cap_type::{Endpoint, Granule, IrqHandler, Notification, Untyped},
    init_thread::{self},
    with_ipc_buffer, with_ipc_buffer_mut, CPtr, CapRights, Error, MessageInfo, ObjectBlueprintArm,
    UntypedDesc, VmAttributes,
};
use sel4_root_task::{debug_println, root_task, Never};
use spin::Mutex;

static TASK_FILES: &[(&str, &[u8])] = &[
    (
        "kernel-thread",
        include_bytes_aligned!(16, "../../../build/kernel-thread.elf"),
    ),
    (
        "block-thread",
        include_bytes_aligned!(16, "../../../build/blk-thread.elf"),
    ),
];

/// The object allocator for the root task.
pub(crate) static OBJ_ALLOCATOR: Mutex<ObjectAllocator> = Mutex::new(ObjectAllocator::empty());

/// free page placeholder
pub(crate) static mut FREE_PAGE_PLACEHOLDER: FreePagePlaceHolder =
    FreePagePlaceHolder([0; GRANULE_SIZE]);

#[root_task(heap_size = 0x10_0000)]
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
        debug_println!(
            "[RootTask] device untyped index range: {:?}",
            bootinfo.device_untyped_range()
        );
        debug_println!(
            "[RootTask] mem untyped index range: {:?}",
            bootinfo.kernel_untyped_range()
        );
        debug_println!(
            "[RootTask] untyped range: {:?}->{:?}",
            bootinfo.untyped().start(),
            bootinfo.untyped().end()
        );
        debug_println!(
            "[RootTask] empty slot range: {:?}",
            bootinfo.empty().range()
        );

        debug_println!("[RootTask] Untyped List: ");
        mem_untypes.iter().rev().for_each(|(index, untyped)| {
            debug_println!(
                "    Untyped({:03}) paddr: {:#x?} size: {:#x}",
                mem_untyped_start + index,
                untyped.paddr(),
                (1usize << untyped.size_bits())
            );
        });
    }

    // Kernel Thread Use the Largest Untyped Memory Region
    let kernel_untyped = CPtr::from_bits(
        (mem_untyped_start
            + mem_untypes
                .pop()
                .expect("[RootTask] No untyped memory for kernel thread")
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
                .expect("[RootTask] No untyped memory for root task")
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

    rebuild_cspace();
    test_threads(&bootinfo);

    let mut tasks = Vec::new();

    // Used for fault and normal IPC ( Reuse )
    let fault_ep = OBJ_ALLOCATOR
        .lock()
        .allocate_and_retyped_fixed_sized::<Endpoint>();
    // Used for IRQ Registration with slot transfer
    let irq_ep = OBJ_ALLOCATOR
        .lock()
        .allocate_and_retyped_fixed_sized::<Endpoint>();
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

    // Prepare Kernel Thread
    {
        tasks[0]
            .abs_cptr(DEFAULT_CUSTOM_SLOT)
            .copy(
                &init_thread::slot::CNODE.cap().relative(kernel_untyped),
                CapRights::all(),
            )
            .unwrap();
    }

    // Prepare Block Thread
    {
        // Channel to send message to block thread
        let blk_dev_ep = OBJ_ALLOCATOR
            .lock()
            .allocate_and_retyped_fixed_sized::<Endpoint>();
        // Set Notification for Blk-Thread Task.
        let net_irq_not = OBJ_ALLOCATOR
            .lock()
            .allocate_and_retyped_fixed_sized::<Notification>();
        tasks[0]
            .abs_cptr(DEFAULT_CUSTOM_SLOT + 1)
            .copy(&utils::abs_cptr(blk_dev_ep), CapRights::all())
            .unwrap();
        tasks[1]
            .abs_cptr(DEFAULT_CUSTOM_SLOT)
            .copy(
                &init_thread::slot::CNODE.cap().relative(net_irq_not),
                CapRights::all(),
            )
            .unwrap();
        tasks[1]
            .abs_cptr(DEFAULT_CUSTOM_SLOT + 2)
            .copy(
                &init_thread::slot::CNODE.cap().relative(blk_dev_ep),
                CapRights::all(),
            )
            .unwrap();
        // Map device memory to blk-thread task
        let (found_device_idx, found_device_desc) = bootinfo.untyped_list()
            [bootinfo.device_untyped_range()]
        .iter()
        .enumerate()
        .find(|(_i, desc)| {
            (desc.paddr()..(desc.paddr() + (1 << desc.size_bits()))).contains(&VIRTIO_MMIO_ADDR)
        })
        .expect("[RootTask] can't find device memory");
        assert!(found_device_desc.is_device());

        let device_untyped_cap = bootinfo.untyped().index(found_device_idx).cap();
        // let device_frame_slot = {
        //     sel4::init_thread::Slot::from_index(OBJ_ALLOCATOR.lock().allocate_slot())
        //         .downcast::<sel4::cap_type::LargePage>()
        // };
        let (device_slot_index, device_cnode_index, device_index) =
            OBJ_ALLOCATOR.lock().allocate_slot();
        let device_frame_slot = sel4::init_thread::Slot::from_index(device_index)
            .downcast::<sel4::cap_type::LargePage>();

        device_untyped_cap
            .untyped_retype(
                &ObjectBlueprintArm::LargePage.into(),
                &init_thread::slot::CNODE
                    .cap()
                    .relative_bits_with_depth(device_cnode_index as u64, 52),
                device_slot_index,
                1,
            )
            .unwrap();

        let device_frame_cap = device_frame_slot.cap();
        // let device_frame_addr = 0x10_2000_0000;
        assert!(device_frame_cap.frame_get_address().unwrap() < VIRTIO_MMIO_ADDR);
        loop {
            match device_frame_cap.frame_map(
                tasks[1].vspace,
                VIRTIO_MMIO_VIRT_ADDR,
                CapRights::all(),
                VmAttributes::DEFAULT,
            ) {
                Ok(()) => {
                    debug_println!("[RootTask] map device memory success");
                    break;
                }
                Err(Error::FailedLookup) => {
                    debug_println!(
                        "[RootTask] map device memory failed, try to allocate page table"
                    );
                    let pt = TaskImpl::allocate_pt(&mut tasks[1]);
                    pt.pt_map(
                        tasks[1].vspace,
                        VIRTIO_MMIO_VIRT_ADDR,
                        VmAttributes::DEFAULT,
                    )
                    .unwrap();
                    tasks[1].mapped_pt.lock().push(pt);
                }
                Err(e) => {
                    panic!("[RootTask] map device memory failed: {:?}", e);
                }
            }
        }
        // Map DMA frame.
        let page_cap = OBJ_ALLOCATOR
            .lock()
            .allocate_and_retyped_fixed_sized::<Granule>();
        tasks[1].map_page(DMA_ADDR_START, page_cap);
        let page_cap = OBJ_ALLOCATOR
            .lock()
            .allocate_and_retyped_fixed_sized::<Granule>();
        tasks[1].map_page(DMA_ADDR_START + PAGE_SIZE, page_cap);
    }

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
                RootMessageLabel::TranslateAddr(addr) => {
                    let phys_addr = tasks[badge as usize]
                        .mapped_page
                        .get(&(addr / 0x1000 * 0x1000));
                    let message = RootMessageLabel::TranslateAddr(
                        phys_addr.unwrap().frame_get_address().unwrap() + addr % 0x1000,
                    )
                    .build();
                    with_ipc_buffer_mut(|buffer| sel4::reply(buffer, message));
                }
            }
        } else {
            let fault = with_ipc_buffer(|buffer| sel4::Fault::new(buffer, &message));
            debug_println!("[RootTask] received fault {:#x?}", fault)
        }
    }
}
