use crate_consts::DEFAULT_CUSTOM_SLOT;
use sel4::{
    cap_type, debug_println, set_ipc_buffer, with_ipc_buffer, BootInfo, CNodeCapData, CapRights,
    IPCBuffer, Notification, UserContext,
};
use task_helper::TaskHelperTrait;

use crate::{
    obj_allocator::{alloc_cap, alloc_cap_size},
    task::{Sel4Task, TaskImpl},
    utils::{abs_cptr, sys_null},
    CNODE_RADIX_BITS,
};

static TLS_BUFFER: [u8; 0x100] = [0u8; 0x100];
pub fn test_stack() {
    let mut stack = [0u8; 0x1001];
    stack[0x1000] = 1;
    unsafe {
        set_ipc_buffer(IPCBuffer::from_ptr(TaskImpl::IPC_BUFFER_ADDR as _));
    }
    debug_println!("Test Stack Successfully!");
    Notification::from_bits(DEFAULT_CUSTOM_SLOT as _).signal();
    loop {}
}

pub fn test_entry() {
    let fault_ep = alloc_cap::<cap_type::Endpoint>();

    let cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let inner_cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let noti = alloc_cap::<cap_type::Notification>();
    let tcb = alloc_cap::<cap_type::TCB>();

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

    let mut task = Sel4Task::new(tcb, cnode, fault_ep, BootInfo::init_thread_vspace(), 0);

    // Copy Notification
    task.abs_cptr(DEFAULT_CUSTOM_SLOT as u64)
        .copy(&abs_cptr(noti), CapRights::all())
        .unwrap();

    // Configure Root Task
    task.configure(2 * CNODE_RADIX_BITS).unwrap();

    // Map stack for the task.
    task.map_stack(0);

    // Init IPC Buffer
    task.init_ipc_buffer();

    // set task priority and max control priority
    task.tcb
        .tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 255, 255)
        .unwrap();

    let mut context = UserContext::default();
    *context.pc_mut() = test_stack as _;
    *context.sp_mut() = crate::task::TaskImpl::DEFAULT_STACK_TOP as _;
    context.inner_mut().tpidr_el0 = TLS_BUFFER.as_ptr() as _;
    task.tcb
        .tcb_write_all_registers(true, &mut context)
        .unwrap();

    let (message, _badge) = fault_ep.recv(());
    let fault = with_ipc_buffer(|buffer| sel4::Fault::new(buffer, &message));
    debug_println!("fault {:#x?}", fault);
    match &fault {
        sel4::Fault::VMFault(fault) => {
            assert!(fault.addr() > 0xffff0000);
            task.map_stack(10);
            task.tcb.tcb_resume().unwrap();
        }
        _ => unreachable!(),
    }
    noti.wait();
    // Drop Task
    task.tcb.tcb_suspend().unwrap();
    task.tcb.debug_name(b"test-missing-page");
    drop(task);
    abs_cptr(cnode).revoke().unwrap();
    abs_cptr(cnode).delete().unwrap();
    abs_cptr(inner_cnode).revoke().unwrap();
    abs_cptr(inner_cnode).delete().unwrap();
    abs_cptr(tcb).revoke().unwrap();
    abs_cptr(tcb).delete().unwrap();
    sys_null(-10);
    debug_println!("Missing Page Handled Successfully ");
    loop {}
}
