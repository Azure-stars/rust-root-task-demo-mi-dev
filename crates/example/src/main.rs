//
// Copyright 2023, Colias Group, LLC
//
// SPDX-License-Identifier: BSD-2-Clause
//

#![no_std]
#![no_main]
#![feature(never_type)]

mod image_utils;
mod object_allocator;
mod tls;
use core::cell::UnsafeCell;

use image_utils::UserImageUtils;
use sel4::{
    set_ipc_buffer, with_ipc_buffer, with_ipc_buffer_mut, r#yield, BootInfo, CNodeCapData, Fault, IPCBuffer, LocalCPtr, MessageInfo, GRANULE_SIZE
};
use sel4_logging::{log, LevelFilter, Logger};
use sel4_root_task::{debug_println, root_task};

#[repr(align(4096))]
pub struct AlignedIPCBuffer(UnsafeCell<[u8; 4096]>);

impl AlignedIPCBuffer {
    fn get_ipc_buffer(&mut self) -> IPCBuffer {
        unsafe { IPCBuffer::from_ptr(self.0.get().cast()) }
    }
}

fn test(ep_bits: u64, ipc_buffer: IPCBuffer) {
    set_ipc_buffer(ipc_buffer);
    debug_println!("set ipc buffer done");

    // debug_snapshot();
    let ep = LocalCPtr::<sel4::cap_type::Endpoint>::from_bits(ep_bits);

    with_ipc_buffer_mut(|buffer| {
        for i in 0..3 {
            buffer.msg_bytes_mut()[i] = i as u8;
        }
    });

    debug_println!("ready for done");
    ep.send(MessageInfo::new(0x1234, 0, 0, 3));

    debug_println!("send done");
    BootInfo::init_thread_tcb().debug_name(b"test_after");
    sys_null(-10);
    debug_println!("send ipc buffer done");
    r#yield();

    unsafe {
        (0x12345678 as *mut u8).write_volatile(0);
    }
    debug_println!("resumed task");
    loop {
        r#yield()
    }
    // unreachable!();
}

static mut RAW_BUFFER: AlignedIPCBuffer =
    AlignedIPCBuffer(UnsafeCell::new([0; GRANULE_SIZE.bytes()]));

#[root_task]
fn main(bootinfo: &sel4::BootInfo) -> sel4::Result<!> {
    static LOGGER: Logger = sel4_logging::LoggerBuilder::const_default()
        .level_filter(LevelFilter::Debug)
        .write(|s| sel4::debug_print!("{}", s))
        .build();
    LOGGER.set().unwrap();
    LOGGER.set_max_level();

    log::debug!("=================== Hello World! =====================");
    sel4::debug_println!("log filter: {:?}", LOGGER.level_filter());
    log::debug!("=================== Hello World! =====================");
    UserImageUtils.init(bootinfo);

    let mut obj_allocator = object_allocator::ObjectAllocator::new(bootinfo);

    let cnode = BootInfo::init_thread_cnode();
    let vspace = BootInfo::init_thread_vspace();

    let ep = obj_allocator.allocate_ep();
    let tcb = obj_allocator.allocate_tcb();

    let buffer_ptr = unsafe { RAW_BUFFER.0.get() } as usize;
    let ipc_buffer = LocalCPtr::<sel4::cap_type::SmallPage>::from_bits(
        UserImageUtils.get_user_image_frame_slot(buffer_ptr) as u64,
    );

    let cnode = obj_allocator.allocate_cnode(12);
    cnode
        .relative_bits_with_depth(ep.cptr().bits(), 12)
        .copy(
            &BootInfo::init_thread_cnode().relative(ep),
            sel4::CapRights::all(),
        )
        .unwrap();

    cnode
        .relative_bits_with_depth(1, 12)
        .copy(
            &BootInfo::init_thread_cnode().relative(tcb),
            sel4::CapRights::all(),
        )
        .unwrap();

    tcb.tcb_configure(
        ep.cptr(),
        cnode,
        CNodeCapData::new(0, sel4::WORD_SIZE - 12),
        vspace,
        buffer_ptr as _,
        ipc_buffer,
    )?;
    tcb.tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 0, 255)?;

    let sp_buffer = [0u8; 0x1000];
    let tp = [0u8; 0x200];
    let mut user_context = sel4::UserContext::default();

    *user_context.pc_mut() = test as u64;
    *user_context.sp_mut() = sp_buffer.as_ptr() as u64 + 0x1000;
    user_context.inner_mut().tpidr_el0 = tp.as_ptr() as u64;
    *user_context.gpr_mut(0) = ep.cptr().bits();
    *user_context.gpr_mut(1) = buffer_ptr as _;

    tcb.tcb_write_all_registers(false, &mut user_context)
        .unwrap();

    tcb.tcb_set_affinity(0).unwrap();
    tcb.debug_name(b"before name");

    // sel4::debug_snapshot();
    tcb.tcb_resume().unwrap();

    loop {
        // sys_null(-10);
        r#yield();
        // sys_null(-10);
        let (message, badge) = ep.recv(());
        debug_println!(
            "recv message: {:#x?} length: {}",
            message.label(),
            message.length()
        );
        
        if message.label() < 0x100 {
            let fault = with_ipc_buffer(|buffer| Fault::new(&buffer, &message));
            if let Fault::VMFault(_) = fault {
                ep.nb_send(MessageInfo::new(0, 0, 0, 0));
                tcb.tcb_resume();
            }
            debug_println!("fault: {:#x?}", fault);
        }

        if message.length() > 0 {
            with_ipc_buffer(|buffer| {
                buffer.msg_bytes()[..message.length()]
                    .iter()
                    .enumerate()
                    .for_each(|(i, x)| debug_println!("bytes: {i} {}", *x));
            });
        }
    }
    sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    unreachable!()
}

pub fn sys_null(sys: isize) {
    unsafe {
        core::arch::asm!("svc 0",
            in("x7") sys,
        );
    }
}
