#![no_std]
#![no_main]
#![feature(naked_functions)]

extern crate sel4_panicking;

use common::CustomMessageLabel;
use core::{
    arch::asm,
    ptr,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};
use sel4::{
    cap::Endpoint, debug_println, set_ipc_buffer, with_ipc_buffer_mut,
    CapTypeForFrameObjectOfFixedSize, MessageInfo,
};

const WORD_SIZE: usize = core::mem::size_of::<usize>();

/// TLS register of shim component, use it to restore in [vsyscall_handler]
static TP_REG: AtomicUsize = AtomicUsize::new(0);
/// Endpoint cptr
static EP_CPTR: AtomicU64 = AtomicU64::new(0);

sel4_panicking_env::register_debug_put_char!(sel4::debug_put_char);

fn main(ep: Endpoint, busybox_entry: usize, vsyscall_section: usize) -> usize {
    {
        debug_println!("[User] busybox entry: {:#x}", busybox_entry);
        debug_println!(
            "[User] vyscall section: {:#x} -> {:#x}",
            vsyscall_section,
            vsyscall_handler as usize
        );
    }

    set_ipc_buffer_with_symbol();

    // Initialize vsyscall
    if vsyscall_section != 0 {
        unsafe {
            (vsyscall_section as *mut usize).write_volatile(vsyscall_handler as usize);
        }
    }

    // Store Tls reg and endpoint cptr
    TP_REG.store(get_tp(), Ordering::SeqCst);
    EP_CPTR.store(ep.bits(), Ordering::SeqCst);

    // Test Send Custom Message
    ep.call(MessageInfo::new(
        CustomMessageLabel::TestCustomMessage.to_label(),
        0,
        0,
        0,
    ));

    debug_println!("[User] send ipc buffer to kernel thread ok");

    // Return the true entry point
    return busybox_entry;
}

pub fn vsyscall_handler(
    syscall_id: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
    arg6: usize,
) -> usize {
    let tp = get_tp();
    // Restore the TLS register used by Shim components.
    set_tp(TP_REG.load(Ordering::SeqCst));

    with_ipc_buffer_mut(|buffer| {
        let msgs = buffer.msg_regs_mut();
        msgs[0] = syscall_id as _;
        msgs[1] = arg1 as _;
        msgs[2] = arg2 as _;
        msgs[3] = arg3 as _;
        msgs[4] = arg4 as _;
        msgs[5] = arg5 as _;
        msgs[6] = arg6 as _;
    });

    let ep = Endpoint::from_bits(EP_CPTR.load(Ordering::SeqCst));
    let msg = ep.call(MessageInfo::new(
        CustomMessageLabel::SysCall.to_label(),
        0,
        0,
        7 * WORD_SIZE,
    ));

    // Ensure that has one WORD_SIZE contains result.
    assert_eq!(msg.length(), WORD_SIZE);

    // Get the result of the fake syscall
    let ret = with_ipc_buffer_mut(|buffer| buffer.msg_regs()[0]);

    // Restore The TLS Register used by linux App
    set_tp(tp);

    ret as _
}

#[no_mangle]
#[naked]
unsafe extern "C" fn _start() -> ! {
    asm!(
        "
            bl      {main}
            mov     x1, 0
            msr     tpidr_el0, x1
            blr     x0
            b       .
        ",
        main = sym main,
        options(noreturn)
    )
}

fn get_tp() -> usize {
    let mut tp: usize;
    unsafe {
        asm!("mrs {}, tpidr_el0", out(reg) tp);
    }
    tp
}

fn set_tp(tp: usize) {
    unsafe {
        asm!("msr tpidr_el0, {}", in(reg) tp);
    }
}

fn set_ipc_buffer_with_symbol() {
    extern "C" {
        static _end: usize;
    }
    let ipc_buffer = unsafe {
        ((ptr::addr_of!(_end) as usize)
            .next_multiple_of(sel4::cap_type::Granule::FRAME_OBJECT_TYPE.bytes())
            as *mut sel4::IpcBuffer)
            .as_mut()
            .unwrap()
    };

    set_ipc_buffer(ipc_buffer);
}

/// Send a syscall to sel4 with none arguments
pub fn syscall0(id: isize) {
    unsafe {
        core::arch::asm!(
            "svc 0",
            in("x7") id,
        );
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    debug_println!("[User] Task Error");
    loop {}
}
