use sel4::{debug_println, BootInfo};

use crate::object_allocator::{allocate_irq_handler, allocate_notification};

const SERIAL_DEVICE_IRQ: usize = 33;

pub fn test_irq() {
    let irq_handler = allocate_irq_handler();
    let notification = allocate_notification();

    BootInfo::irq_control()
        .irq_control_get(
            SERIAL_DEVICE_IRQ as _,
            &BootInfo::init_thread_cnode().relative(irq_handler),
        )
        .unwrap();
    irq_handler
        .irq_handler_set_notification(notification)
        .unwrap();

    irq_handler.irq_handler_ack().unwrap();

    debug_println!("[Kernel Thread] Waiting for irq notification");
    notification.wait();
    debug_println!("[Kernel Thread] Received irq notification");
}
