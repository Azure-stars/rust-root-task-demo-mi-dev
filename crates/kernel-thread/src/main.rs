#![no_std]
#![no_main]
#![feature(never_type)]
#![feature(const_trait_impl)]
#![feature(effects)]

extern crate alloc;
extern crate sel4_panicking;

mod child;
mod irq_test;
mod logging;
mod object_allocator;
mod syscall;
mod task;
mod utils;

use crate_consts::{DEFAULT_CUSTOM_SLOT, DEFAULT_EMPTY_SLOT_INDEX, KERNEL_THREAD_SLOT_NUMS};
use object_allocator::{alloc_cap, OBJ_ALLOCATOR};
use sel4::{cap_type, debug_println, set_ipc_buffer, IPCBuffer, LocalCPtr};
use sel4_sys::seL4_DebugPutChar;

sel4_panicking_env::register_debug_put_char!(seL4_DebugPutChar);

// Define heap allocator and add default memory.
const DEFAULT_ALLOCATOR_SIZE: usize = 0x1_0000;
alloc_helper::define_allocator! {
    /// Define a new global allocator
    (GLOBAL_ALLOCATOR, DEFAULT_ALLOCATOR_SIZE)
}

/// Get the virtual address of the page seat.
pub fn page_seat_vaddr() -> usize {
    0x1_0000_2000
}

#[export_name = "_start"]
fn main(ipc_buffer: IPCBuffer) -> sel4::Result<!> {
    logging::init();
    set_ipc_buffer(ipc_buffer);
    OBJ_ALLOCATOR.lock().init(
        DEFAULT_EMPTY_SLOT_INDEX..KERNEL_THREAD_SLOT_NUMS - 2,
        LocalCPtr::from_bits(DEFAULT_CUSTOM_SLOT as _),
    );
    // test_func!("Test IRQ", irq_test::test_irq());
    test_func!("Test IRQ", irq_test::test_irq_with_cap_transfer());

    test_func!("Test Thread", {
        let ep = alloc_cap::<cap_type::Endpoint>();
        child::test_child(ep).unwrap()
    });

    sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    unreachable!()
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    debug_println!("Task Error");
    loop {}
}
