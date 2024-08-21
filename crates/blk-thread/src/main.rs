#![no_std]
#![no_main]
#![feature(never_type)]

// extern crate alloc;
extern crate sel4_panicking;

mod virtio_impl;

use core::ptr::NonNull;

use alloc_helper::defind_allocator;
use common::RootMessageLabel;
use sel4::{cap_type, debug_println, set_ipc_buffer, IPCBuffer, IRQHandler, LocalCPtr, Notification};
use virtio_drivers::{
    device::blk::{BlkReq, BlkResp, VirtIOBlk},
    transport::mmio::{MmioTransport, VirtIOHeader},
};
use virtio_impl::HalImpl;

sel4_panicking_env::register_debug_put_char!(sel4::sys::seL4_DebugPutChar);

/// Get the virtual address of the page seat.
pub fn page_seat_vaddr() -> usize {
    0x1_0000_2000
}

const VIRTIO_BLK_ADDR: usize = 0x1_2000_3e00;

/// Default size of the global allocator
const DEFAULT_ALLOCATOR_SIZE: usize = 0x8000;
defind_allocator! {
    /// Define a new global allocator
    /// Size is [DEFAULT_ALLOCATOR_SIZE]
    (GLOBAL_ALLOCATOR, DEFAULT_ALLOCATOR_SIZE)
}

const VIRTIO_NET_IRQ: usize = 0x2f + 0x20;

#[export_name = "_start"]
fn main(ipc_buffer: IPCBuffer) -> sel4::Result<!> {
    set_ipc_buffer(ipc_buffer);
    debug_println!("[Blk Thread] Blk-Thread");

    let mut virtio_blk = VirtIOBlk::<HalImpl, MmioTransport>::new(unsafe {
        MmioTransport::new(NonNull::new(VIRTIO_BLK_ADDR as *mut VirtIOHeader).unwrap()).unwrap()
    })
    .expect("failed to create blk driver");

    debug_println!("[Blk Thread] Block device capacity: {:#x}", virtio_blk.capacity());

    let mut request = BlkReq::default();
    let mut resp = BlkResp::default();
    let mut token = 0;
    let mut buffer = [0u8; 512];

    // Register interrupt handler and notification
    let irq_handler = IRQHandler::from_bits(20);
    let irq_notify = Notification::from_bits(19);
    let ep = LocalCPtr::<cap_type::Endpoint>::from_bits(18);

    ep.call(RootMessageLabel::RegisterIRQ(irq_handler.bits(), VIRTIO_NET_IRQ as _).build());

    irq_handler
        .irq_handler_set_notification(irq_notify)
        .unwrap();

    irq_handler.irq_handler_ack().unwrap();

    // Read block device
    token = unsafe {
        virtio_blk.read_blocks_nb(0, &mut request, &mut buffer, &mut resp)
            .unwrap()
    };

    debug_println!("[Blk Thread] Waiting for VIRTIO Net IRQ notification");
    irq_notify.wait();
    irq_handler.irq_handler_ack().unwrap();
    virtio_blk.ack_interrupt();
    debug_println!("[Blk Thread] Received for VIRTIO Net IRQ notification");

    debug_println!("[Blk Thread] Read done");
    debug_println!("[Blk Thread] Data 0..4: {:?}", &buffer[0..4]);

    debug_println!();

    // Read block device
    token = unsafe {
        virtio_blk.read_blocks_nb(1, &mut request, &mut buffer, &mut resp)
            .unwrap()
    };

    debug_println!("[Blk Thread] Waiting for VIRTIO Net IRQ notification");
    irq_notify.wait();
    irq_handler.irq_handler_ack().unwrap();
    virtio_blk.ack_interrupt();
    debug_println!("[Blk Thread] Received for VIRTIO Net IRQ notification");


    // virtio_blk.read_blocks(0, &mut buffer).unwrap();

    debug_println!("[Blk Thread] Read done");
    debug_println!("[Blk Thread] Data 0..4: {:?}", &buffer[0..4]);

    sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    unreachable!()
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    debug_println!("Task Error");
    loop {}
}
