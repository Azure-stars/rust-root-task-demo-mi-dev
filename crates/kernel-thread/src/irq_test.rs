use core::mem::size_of;

use sel4::{cap_type, debug_println, with_ipc_buffer_mut, LocalCPtr, MessageInfo};

use crate::object_allocator::{allocate_irq_handler, allocate_notification};

const SERIAL_DEVICE_IRQ: usize = 33;

pub fn test_irq() {
    let irq_handler = allocate_irq_handler();
    let notification = allocate_notification();
    let ep = LocalCPtr::<cap_type::Endpoint>::from_bits(18);
    with_ipc_buffer_mut(|buffer| {
        buffer.msg_regs_mut()[0] = irq_handler.bits();
        buffer.msg_regs_mut()[1] = SERIAL_DEVICE_IRQ as _;
    });
    let message = MessageInfo::new(0x10, 0, 0, 2 * size_of::<u64>());
    ep.call(message);
    // BootInfo::irq_control()
    //     .irq_control_get(
    //         SERIAL_DEVICE_IRQ as _,
    //         &BootInfo::init_thread_cnode().relative(irq_handler),
    //     )
    //     .unwrap();
    irq_handler
        .irq_handler_set_notification(notification)
        .unwrap();

    irq_handler.irq_handler_ack().unwrap();

    debug_println!("[Kernel Thread] Waiting for irq notification");
    notification.wait();
    debug_println!("[Kernel Thread] Received irq notification");
}
