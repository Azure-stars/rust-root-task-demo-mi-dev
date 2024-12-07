#![no_std]
#![no_main]
#![feature(never_type)]
#![feature(const_trait_impl)]

extern crate alloc;
extern crate sel4_panicking;

mod child_test;
mod ipc;
mod irq_test;
mod logging;
mod runtime;
mod task;
mod utils;

use common::ObjectAllocator;
use crate_consts::{
    DEFAULT_CUSTOM_SLOT, DEFAULT_EMPTY_SLOT_INDEX, GRANULE_SIZE, KERNEL_THREAD_SLOT_NUMS,
};
use sel4::{cap_type::Endpoint, debug_println, Cap};
use sel4_sys::seL4_DebugPutChar;
use spin::Mutex;
use utils::{init_free_page_addr, FreePagePlaceHolder};

sel4_panicking_env::register_debug_put_char!(seL4_DebugPutChar);

/// Get the virtual address of the page seat.
pub fn page_seat_vaddr() -> usize {
    unsafe { init_free_page_addr() }
}

/// The object allocator for the kernel thread.
pub(crate) static OBJ_ALLOCATOR: Mutex<ObjectAllocator> = Mutex::new(ObjectAllocator::empty());

/// free page placeholder
pub(crate) static mut FREE_PAGE_PLACEHOLDER: FreePagePlaceHolder =
    FreePagePlaceHolder([0; GRANULE_SIZE]);

fn main() -> ! {
    debug_println!("[KernelThread] EntryPoint");
    logging::init();
    OBJ_ALLOCATOR.lock().init(
        DEFAULT_EMPTY_SLOT_INDEX..KERNEL_THREAD_SLOT_NUMS,
        Cap::from_bits(DEFAULT_CUSTOM_SLOT as _),
    );
    debug_println!("[KernelThread] Object Allocator initialized");
    // test_func!("Test IRQ", irq_test::test_irq());
    test_func!(
        "[KernelThread] Test IRQ",
        irq_test::test_irq_with_cap_transfer()
    );

    test_func!("[KernelThread] Test Thread", {
        let ep = OBJ_ALLOCATOR
            .lock()
            .allocate_and_retyped_fixed_sized::<Endpoint>();
        child_test::test_child(ep).unwrap()
    });
    debug_println!("[KernelThread] Say Goodbye");
    sel4::cap::Tcb::from_bits(1).tcb_suspend().unwrap();
    unreachable!()
}
