#![no_std]
#![no_main]

use core::ptr::NonNull;

use common::{RootMessageLabel, VIRTIO_MMIO_BLK_VIRT_ADDR};
use crate_consts::{DEFAULT_CUSTOM_SLOT, DEFAULT_THREAD_FAULT_EP, VIRTIO_NET_IRQ};
use sel4::{
    cap::{IrqHandler, Notification},
    cap_type::Endpoint,
    debug_println, init_thread, Cap,
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

pub fn fmt_with_module(record: &log::Record, f: &mut core::fmt::Formatter) -> core::fmt::Result {
    let target = match record.target().is_empty() {
        true => record.module_path().unwrap_or_default(),
        false => record.target(),
    };
    let color_code = match record.level() {
        log::Level::Error => 31u8, // Red
        log::Level::Warn => 93,    // BrightYellow
        log::Level::Info => 34,    // Blue
        log::Level::Debug => 32,   // Green
        log::Level::Trace => 90,   // BrightBlack
    };

    write!(
        f,
        "\u{1B}[{}m\
            [{}] [{}] {}\
            \u{1B}[0m",
        color_code,
        record.level(),
        target,
        record.args()
    )
}

fn main() -> ! {
    static LOGGER: sel4_logging::Logger = sel4_logging::LoggerBuilder::const_default()
        .write(|s| sel4::debug_print!("{}", s))
        .level_filter(log::LevelFilter::Trace)
        .fmt(fmt_with_module)
        .build();
    LOGGER.set().unwrap();
    debug_println!("[BlockThread] EntryPoint");
    let mut virtio_blk = VirtIOBlk::<HalImpl, MmioTransport>::new(unsafe {
        MmioTransport::new(NonNull::new(VIRTIO_MMIO_BLK_VIRT_ADDR as *mut VirtIOHeader).unwrap())
            .unwrap()
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

    for block_id in 0..2 {
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
    init_thread::slot::TCB.cap().tcb_suspend().unwrap();
    unreachable!()
}
