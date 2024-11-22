#![no_std]
#![no_main]

use core::ptr::NonNull;

use common::{RootMessageLabel, VIRTIO_MMIO_ADDR};
use crate_consts::{DEFAULT_CUSTOM_SLOT, DEFAULT_THREAD_FAULT_EP, VIRTIO_NET_IRQ};
use sel4::{
    cap::{IrqHandler, Notification},
    cap_type::Endpoint,
    debug_println, Cap,
};
use virtio::HalImpl;
use virtio_drivers::{
    device::blk::{BlkReq, BlkResp, VirtIOBlk},
    transport::mmio::{MmioTransport, VirtIOHeader},
};

extern crate sel4_panicking;

mod runtime;
mod virtio;

sel4_panicking_env::register_debug_put_char!(sel4::sys::seL4_DebugPutChar);

fn main() -> ! {
    debug_println!("[BlockThread] EntryPoint");
    let mut virtio_blk = VirtIOBlk::<HalImpl, MmioTransport>::new(unsafe {
        MmioTransport::new(NonNull::new(VIRTIO_MMIO_ADDR as *mut VirtIOHeader).unwrap()).unwrap()
    })
    .expect("[BlockThread] failed to create blk driver");

    debug_println!(
        "[BlockThread] Block device capacity: {:#x}",
        virtio_blk.capacity()
    );

    // Register interrupt handler and notification
    let ntfn = Notification::from_bits(DEFAULT_CUSTOM_SLOT);
    let irq_handler = IrqHandler::from_bits(DEFAULT_CUSTOM_SLOT + 1);
    let ep = Cap::<Endpoint>::from_bits(DEFAULT_THREAD_FAULT_EP);

    ep.call(RootMessageLabel::RegisterIRQ(irq_handler.bits(), VIRTIO_NET_IRQ as _).build());
    irq_handler.irq_handler_set_notification(ntfn).unwrap();
    irq_handler.irq_handler_ack().unwrap();

    // Read block device
    let mut request = BlkReq::default();
    let mut resp = BlkResp::default();
    let mut buffer = [0u8; 512];

    for block_id in 0..1 {
        unsafe {
            virtio_blk
                .read_blocks_nb(block_id, &mut request, &mut buffer, &mut resp)
                .unwrap()
        };

        debug_println!("[BlockThread] Waiting for VIRTIO Net IRQ notification");
        ntfn.wait();
        irq_handler.irq_handler_ack().unwrap();
        virtio_blk.ack_interrupt();
        debug_println!("[BlockThread] Received for VIRTIO Net IRQ notification");

        debug_println!(
            "[BlockThread] Get Data Len: {}, 0..4: {:?}",
            buffer.len(),
            &buffer[0..4]
        );
    }

    debug_println!("[BlockThread] Say Goodbye");
    sel4::cap::Tcb::from_bits(1).tcb_suspend().unwrap();
    unreachable!()
}
