use common::RootMessageLabel;
use crate_consts::{DEFAULT_THREAD_FAULT_EP, DEFAULT_THREAD_IRQ_EP, DEFAULT_THREAD_RECV_SLOT};
use sel4::{
    cap_type::{self},
    debug_println, reply, with_ipc_buffer_mut, BootInfo, CPtr, IRQHandler, LocalCPtr, MessageInfo,
};

use crate::object_allocator::alloc_cap;

const SERIAL_DEVICE_IRQ: usize = 33;

pub fn test_irq() {
    let ep = LocalCPtr::<cap_type::Endpoint>::from_bits(DEFAULT_THREAD_FAULT_EP);
    let notification = alloc_cap::<cap_type::Notification>();
    let irq_handler = alloc_cap::<cap_type::IRQHandler>();

    ep.call(RootMessageLabel::RegisterIRQ(irq_handler.bits(), SERIAL_DEVICE_IRQ as _).build());

    irq_handler
        .irq_handler_set_notification(notification)
        .unwrap();
    irq_handler.irq_handler_ack().unwrap();

    debug_println!("[Kernel Thread] Waiting for irq notification");
    notification.wait();
    debug_println!("[Kernel Thread] Received irq notification");

    irq_handler.irq_handler_clear().unwrap();
}

pub fn test_irq_with_cap_transfer() {
    let ep = LocalCPtr::<cap_type::Endpoint>::from_bits(DEFAULT_THREAD_FAULT_EP);
    let irq_ep = LocalCPtr::<cap_type::Endpoint>::from_bits(DEFAULT_THREAD_IRQ_EP);
    let irq_handler = IRQHandler::from_bits(DEFAULT_THREAD_RECV_SLOT as _);
    let notification = alloc_cap::<cap_type::Notification>();

    ep.call(RootMessageLabel::RegisterIRQWithCap(SERIAL_DEVICE_IRQ as _).build());

    // Set the recv slot for the irq_ep
    with_ipc_buffer_mut(|buffer| {
        buffer.set_recv_slot(
            &BootInfo::init_thread_cnode().relative(CPtr::from_bits(DEFAULT_THREAD_RECV_SLOT as _)),
        )
    });
    irq_ep.recv(());
    with_ipc_buffer_mut(|buffer| reply(buffer, MessageInfo::new(0, 0, 0, 0)));

    irq_handler
        .irq_handler_set_notification(notification)
        .unwrap();
    irq_handler.irq_handler_ack().unwrap();

    debug_println!("[Kernel Thread] Waiting for irq notification");
    notification.wait();
    debug_println!("[Kernel Thread] Received irq notification");
}
