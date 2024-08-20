#![no_std]
#![no_main]
#![feature(never_type)]

// extern crate alloc;
extern crate sel4_panicking;

use sel4::{debug_println, set_ipc_buffer, IPCBuffer};
use sel4_sys::seL4_DebugPutChar;

sel4_panicking_env::register_debug_put_char!(seL4_DebugPutChar);

/// Get the virtual address of the page seat.
pub fn page_seat_vaddr() -> usize {
    0x1_0000_2000
}

#[export_name = "_start"]
fn main(ipc_buffer: IPCBuffer) -> sel4::Result<!> {
    set_ipc_buffer(ipc_buffer);

    sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    unreachable!()
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    debug_println!("Task Error");
    loop {}
}
