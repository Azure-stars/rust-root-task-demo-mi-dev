#![no_std]
#![no_main]
#![feature(naked_functions)]

extern crate sel4_panicking;

sel4_panicking_env::register_debug_put_char!(sel4::sys::seL4_DebugPutChar);

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use common::CustomMessageLabel;
use sel4::{debug_println, set_ipc_buffer, with_ipc_buffer_mut, Endpoint, IPCBuffer, MessageInfo};
use syscalls::Sysno;

/// Load current tls register.
pub(crate) fn load_tp_reg() -> usize {
    let mut tp: usize;
    unsafe {
        core::arch::asm!("mrs {0}, tpidr_el0", out(reg) tp);
    }
    tp
}

/// Save the tls register
pub(crate) fn set_tp_reg(tp: usize) {
    unsafe {
        core::arch::asm!("msr tpidr_el0, {0}", in(reg) tp);
    }
}
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

const WORD_SIZE: usize = core::mem::size_of::<usize>();

/// vsyscall handler.
pub fn vsyscall_handler(
    id: usize,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
) -> usize {
    let tp = load_tp_reg();
    // Restore the TLS register used by Shim components.
    set_tp_reg(TP_REG.load(Ordering::SeqCst));

    // Write syscall registers to ipc buffer.
    with_ipc_buffer_mut(|buffer| {
        let msgs = buffer.msg_regs_mut();
        msgs[0] = id as _;
        msgs[1] = a as _;
        msgs[2] = b as _;
        msgs[3] = c as _;
        msgs[4] = d as _;
        msgs[5] = e as _;
        msgs[6] = f as _;
    });
    // Load endpoint and send SysCall message.
    let ep = Endpoint::from_bits(EP_CPTR.load(Ordering::SeqCst));
    let message = ep.call(MessageInfo::new(
        CustomMessageLabel::SysCall.to_label(),
        0,
        0,
        7 * WORD_SIZE,
    ));

    // Ensure that has one WORD_SIZE contains result.
    assert_eq!(message.length(), WORD_SIZE);

    // Get the result of the fake syscall
    let ret = with_ipc_buffer_mut(|buffer| buffer.msg_regs()[0]);

    // Restore The TLS Register used by linux App
    set_tp_reg(tp);

    ret as usize
}

/// TLS register of shim component, use it to restore in [vsyscall_handler]
pub(crate) static TP_REG: AtomicUsize = AtomicUsize::new(0);
/// Endpoint cptr
pub(crate) static EP_CPTR: AtomicU64 = AtomicU64::new(0);

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

    // // Initialize vsyscall
    // if vsyscall_section != 0 {
    //     unsafe {
    //         (vsyscall_section as *mut usize).write_volatile(vsyscall_handler as usize);
    //     }
    // }

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

    let mmap_ptr = vsyscall_handler(
        Sysno::mmap.id() as usize,
        0x1000,
        0x1000,
        0b11,
        0b10000,
        0,
        0,
    );

    let content = "Hello, World!";

    // unsafe {
    //     core::ptr::write(mmap_ptr as *mut u8, content.as_ptr().read());
    // }
    unsafe {
        core::ptr::copy_nonoverlapping(content.as_ptr(), mmap_ptr as *mut u8, content.len());
    }
    let _ = vsyscall_handler(
        Sysno::write.id() as usize,
        1,
        mmap_ptr as usize,
        content.len(),
        0,
        0,
        0,
    );

    let _ = vsyscall_handler(Sysno::execve.id() as usize, 0, 0, 0, 0, 0, 0);

    return 0;
    // // Return the true entry point
    // return busybox_entry;
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
