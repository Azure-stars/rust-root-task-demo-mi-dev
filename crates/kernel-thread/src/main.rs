#![no_std]
#![no_main]
#![feature(never_type)]
#![feature(const_trait_impl)]

extern crate alloc;
extern crate sel4_panicking;

mod runtime;
// mod child;
// mod ipc_call;
// mod irq_test;
mod logging;
mod object_allocator;
// mod task;
mod utils;

use crate_consts::{DEFAULT_CUSTOM_SLOT, DEFAULT_EMPTY_SLOT_INDEX, KERNEL_THREAD_SLOT_NUMS};
use object_allocator::OBJ_ALLOCATOR;
use sel4::{cap_type, debug_println, init_thread, Cap};
use sel4_sys::seL4_DebugPutChar;

sel4_panicking_env::register_debug_put_char!(seL4_DebugPutChar);

// // Define heap allocator and add default memory.
// const DEFAULT_ALLOCATOR_SIZE: usize = 0x1_0000;
// alloc_helper::define_allocator! {
//     /// Define a new global allocator
//     (GLOBAL_ALLOCATOR, DEFAULT_ALLOCATOR_SIZE)
// }

/// Get the virtual address of the page seat.
pub fn page_seat_vaddr() -> usize {
    0x1_0000_2000
}

fn main() -> ! {
    debug_println!("[kernel Thread] Start");
    logging::init();
    OBJ_ALLOCATOR.lock().init(
        DEFAULT_EMPTY_SLOT_INDEX..KERNEL_THREAD_SLOT_NUMS - 2,
        Cap::from_bits(DEFAULT_CUSTOM_SLOT as _),
    );
    // test_func!("Test IRQ", irq_test::test_irq());
    // test_func!("Test IRQ", irq_test::test_irq_with_cap_transfer());

    // test_func!("Test Thread", {
    //     let ep = alloc_cap::<cap_type::Endpoint>();
    //     child::test_child(ep).unwrap()
    // });
    debug_println!("[kernel-thread] Say Goodbye");
    sel4::cap::Notification::from_bits(1).signal();
    // sel4::init_thread::slot::TCB.cap().tcb_suspend().unwrap();
    sel4::cap::Tcb::from_bits(2).tcb_suspend().unwrap();
    unreachable!()
}
