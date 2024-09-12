use common::RootMessageLabel;
use crate_consts::{
    DEFAULT_CUSTOM_SLOT, DEFAULT_THREAD_FAULT_EP, DEFAULT_THREAD_IRQ_EP, DEFAULT_THREAD_RECV_SLOT,
};
use sel4::{
    cap_type::{self},
    debug_println, reply, with_ipc_buffer, with_ipc_buffer_mut, AbsoluteCPtr, BootInfo, CNode,
    CPtr, CapRights, IRQHandler, LocalCPtr, MessageInfo,
};

use sel4_sys::seL4_MessageInfo;

use crate::object_allocator::alloc_cap;

const SERIAL_DEVICE_IRQ: usize = 33;

pub fn test_irq() {
    let ep = LocalCPtr::<cap_type::Endpoint>::from_bits(DEFAULT_THREAD_FAULT_EP);
    let notification = alloc_cap::<cap_type::Notification>();
    let irq_handler = alloc_cap::<cap_type::IRQHandler>();

    let msg =
        ep.call(RootMessageLabel::RegisterIRQ(irq_handler.bits(), SERIAL_DEVICE_IRQ as _).build());
    debug_println!("[kernel thread] get irq register msg: {:?}", msg);

    irq_handler
        .irq_handler_set_notification(notification)
        .unwrap();
    irq_handler.irq_handler_ack().unwrap();

    debug_println!("[Kernel Thread] Waiting for irq notification");
    notification.wait();
    debug_println!("[Kernel Thread] Received irq notification");

    irq_handler.irq_handler_clear().unwrap();
}

pub fn test_irq2() {
    let ep = LocalCPtr::<cap_type::Endpoint>::from_bits(DEFAULT_THREAD_FAULT_EP);
    let notification = alloc_cap::<cap_type::Notification>();

    let msg = ep.call(RootMessageLabel::RegisterIRQWithCap(SERIAL_DEVICE_IRQ as _).build());
    debug_println!("get msg: {:?}", msg);

    let irq_ep = LocalCPtr::<cap_type::Endpoint>::from_bits(DEFAULT_THREAD_IRQ_EP);

    // let root_cnode = CNode::from_bits(190);
    // let irq_handler = alloc_cap::<cap_type::IRQHandler>();
    // let recv_slot = root_cnode.relative(irq_handler.cptr());
    let recv_slot = BootInfo::init_thread_cnode().relative(CPtr::from_bits(888));
    with_ipc_buffer_mut(|buffer| buffer.set_recv_slot(&recv_slot));
    let msg = irq_ep.recv(());
    debug_println!("get msg: {:?}", msg.0);
    loop {}
    with_ipc_buffer_mut(|buffer| reply(buffer, MessageInfo::new(0, 0, 0, 0)));

    let irq_handler = IRQHandler::from_bits(888);

    irq_handler
        .irq_handler_set_notification(notification)
        .unwrap();
    irq_handler.irq_handler_ack().unwrap();

    debug_println!("[Kernel Thread] Waiting for irq notification");
    notification.wait();
    debug_println!("[Kernel Thread] Received irq notification");
}
