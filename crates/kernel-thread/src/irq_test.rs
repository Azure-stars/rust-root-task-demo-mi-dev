use crate::OBJ_ALLOCATOR;
use common::RootMessageLabel;
use crate_consts::{
    DEFAULT_THREAD_FAULT_EP, DEFAULT_THREAD_IRQ_EP, DEFAULT_THREAD_RECV_SLOT, SERIAL_DEVICE_IRQ,
};
use sel4::{
    cap_type::{IrqHandler, Notification},
    init_thread::{self},
    with_ipc_buffer_mut, MessageInfo,
};
use sel4_panicking_env::debug_println;

pub fn test_irq() {
    let irq_handler = OBJ_ALLOCATOR.lock().allocate_normal_cap::<IrqHandler>();
    let ntfn = OBJ_ALLOCATOR.lock().allocate_fixed_sized::<Notification>();
    let ep = sel4::cap::Endpoint::from_bits(DEFAULT_THREAD_FAULT_EP);

    ep.call(RootMessageLabel::RegisterIRQ(irq_handler.bits(), SERIAL_DEVICE_IRQ as _).build());
    irq_handler.irq_handler_set_notification(ntfn).unwrap();

    debug_println!("[Kernel Thread] Waiting for irq notification");
    ntfn.wait();
    debug_println!("[Kernel Thread] Received irq notification");
}

pub fn test_irq_with_cap_transfer() {
    let ep = sel4::cap::Endpoint::from_bits(DEFAULT_THREAD_FAULT_EP);
    let irq_ep = sel4::cap::Endpoint::from_bits(DEFAULT_THREAD_IRQ_EP);
    let irq_handler = sel4::cap::IrqHandler::from_bits(DEFAULT_THREAD_RECV_SLOT);
    let notification = OBJ_ALLOCATOR.lock().allocate_fixed_sized::<Notification>();

    ep.call(RootMessageLabel::RegisterIRQWithCap(SERIAL_DEVICE_IRQ as _).build());

    // Set the recv slot for the irq_ep
    with_ipc_buffer_mut(|buffer| {
        buffer.set_recv_slot(&init_thread::slot::CNODE.cap().relative(irq_handler))
    });
    irq_ep.recv(());
    with_ipc_buffer_mut(|buffer| sel4::reply(buffer, MessageInfo::new(0, 0, 0, 0)));

    irq_handler
        .irq_handler_set_notification(notification)
        .unwrap();
    irq_handler.irq_handler_ack().unwrap();

    debug_println!("[Kernel Thread] Waiting for irq notification");
    notification.wait();
    debug_println!("[Kernel Thread] Received irq notification");
}
