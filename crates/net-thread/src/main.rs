#![no_std]
#![no_main]
#![feature(never_type)]

// extern crate alloc;
extern crate sel4_panicking;

mod virtio_impl;

use core::ptr::NonNull;

use alloc_helper::defind_allocator;
use common::RootMessageLabel;
use crate_consts::DEFAULT_CUSTOM_SLOT;
use sel4::{
    cap_type, debug_println, set_ipc_buffer, IPCBuffer, IRQHandler, LocalCPtr, Notification,
};
use virtio_drivers::{
    device::net::VirtIONet,
    transport::mmio::{MmioTransport, VirtIOHeader},
};
use virtio_impl::HalImpl;

sel4_panicking_env::register_debug_put_char!(sel4::sys::seL4_DebugPutChar);

const VIRTIO_NET_ADDR: usize = 0x1_2000_3c00;

/// Default size of the global allocator
const DEFAULT_ALLOCATOR_SIZE: usize = 0x1_8000;
defind_allocator! {
    /// Define a new global allocator
    /// Size is [DEFAULT_ALLOCATOR_SIZE]
    (GLOBAL_ALLOCATOR, DEFAULT_ALLOCATOR_SIZE)
}

const VIRTIO_NET_IRQ: usize = 0x2e + 0x20;

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

#[export_name = "_start"]
fn main(ipc_buffer: IPCBuffer) -> sel4::Result<!> {
    static LOGGER: sel4_logging::Logger = sel4_logging::LoggerBuilder::const_default()
        .write(|s| sel4::debug_print!("{}", s))
        .level_filter(log::LevelFilter::Trace)
        .fmt(fmt_with_module)
        .build();
    LOGGER.set().unwrap();
    set_ipc_buffer(ipc_buffer);
    debug_println!("[Net Thread] Net-Thread");

    let mut virtio_net = VirtIONet::<HalImpl, MmioTransport, 32>::new(
        unsafe {
            MmioTransport::new(NonNull::new(VIRTIO_NET_ADDR as *mut VirtIOHeader).unwrap()).unwrap()
        },
        2048,
    )
    .expect("failed to create net driver");

    debug_println!(
        "[Net Thread] Net device mac address: {:?}",
        virtio_net.mac_address()
    );

    let mut tx_buffer = virtio_net.new_tx_buffer(0x200);

    for i in 0..100 {
        tx_buffer.packet_mut()[i] = i as _;
    }

    // Register interrupt handler and notification
    let irq_notify = Notification::from_bits(DEFAULT_CUSTOM_SLOT);
    let irq_handler = IRQHandler::from_bits(DEFAULT_CUSTOM_SLOT + 1);
    let ep = LocalCPtr::<cap_type::Endpoint>::from_bits(18);

    ep.call(RootMessageLabel::RegisterIRQ(irq_handler.bits(), VIRTIO_NET_IRQ as _).build());

    irq_handler.irq_handler_ack().unwrap();

    irq_handler
        .irq_handler_set_notification(irq_notify)
        .unwrap();

    // virtio_net.send(tx_buffer).unwrap();

    debug_println!("[Net Thread] Waiting for VIRTIO Net IRQ notification");
    irq_notify.wait();
    irq_handler.irq_handler_ack().unwrap();
    virtio_net.ack_interrupt();
    debug_println!("[Net Thread] Received for VIRTIO Net IRQ notification");

    sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    unreachable!()
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    debug_println!("Task Error");
    loop {}
}
