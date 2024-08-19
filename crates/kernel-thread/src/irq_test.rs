use common::RootMessageLabel;
use sel4::{cap_type, debug_println, LocalCPtr};

use crate::object_allocator::alloc_cap;

const SERIAL_DEVICE_IRQ: usize = 33;

pub fn test_irq() {
    let irq_handler = alloc_cap::<cap_type::IRQHandler>();
    let notification = alloc_cap::<cap_type::Notification>();
    let ep = LocalCPtr::<cap_type::Endpoint>::from_bits(18);

    ep.call(RootMessageLabel::RegisterIRQ(irq_handler.bits(), SERIAL_DEVICE_IRQ as _).build());

    irq_handler
        .irq_handler_set_notification(notification)
        .unwrap();

    irq_handler.irq_handler_ack().unwrap();

    debug_println!("[Kernel Thread] Waiting for irq notification");
    notification.wait();
    debug_println!("[Kernel Thread] Received irq notification");
}
