#![no_std]
#![no_main]
#![feature(naked_functions)]

mod vsyscall;

use core::sync::atomic::Ordering;

use common::CustomMessageLabel;
use sel4::{debug_println, set_ipc_buffer, Endpoint, IPCBuffer, MessageInfo};

use vsyscall::{load_tp_reg, vsyscall_handler, EP_CPTR, TP_REG};

extern crate sel4_panicking;

sel4_panicking_env::register_debug_put_char!(sel4::debug_put_char);

/// The entry of the shim component.
#[no_mangle]
#[naked]
unsafe extern "C" fn _start() -> () {
    core::arch::asm!(
        "
            bl     {main}
            mov    x1, 0
            msr    tpidr_el0, x1
            blr    x0
            b      .
        ",
        main = sym main,
        options(noreturn)
    )
}

/// The main entry of the shim component
fn main(
    ep: Endpoint,
    ipc_buffer: IPCBuffer,
    busybox_entry: usize,
    vsyscall_section: usize,
) -> usize {
    // Display Debug information
    debug_println!("[User] ipc buffer addr: {:p}", ipc_buffer.ptr());
    debug_println!("[User] busybox entry: {:#x}", busybox_entry);
    debug_println!(
        "[User] vyscall section: {:#x} -> {:#x}",
        vsyscall_section,
        vsyscall_handler as usize
    );
    // Initialize IPC Buffer.
    set_ipc_buffer(ipc_buffer);

    // Initialize vsyscall
    if vsyscall_section != 0 {
        unsafe {
            (vsyscall_section as *mut usize).write_volatile(vsyscall_handler as usize);
        }
    }

    // Store Tls reg and endpoint cptr
    TP_REG.store(load_tp_reg(), Ordering::SeqCst);
    EP_CPTR.store(ep.bits(), Ordering::SeqCst);

    // Test Send Custom Message
    ep.call(MessageInfo::new(
        CustomMessageLabel::TestCustomMessage.to_label(),
        0,
        0,
        0,
    ));

    debug_println!("[User] send ipc buffer done");

    // Return the true entry point
    return busybox_entry;
}

/// Send a syscall to sel4 with none arguments
pub fn sys_null(sys: isize) {
    unsafe {
        core::arch::asm!(
            "svc 0",
            in("x7") sys,
        );
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    debug_println!("Task Error");
    loop {}
}
