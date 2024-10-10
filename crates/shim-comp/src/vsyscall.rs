use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use common::CustomMessageLabel;
use sel4::{with_ipc_buffer_mut, Endpoint, MessageInfo};

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

/// TLS register of shim component, use it to restore in [vsyscall_handler]
pub(crate) static TP_REG: AtomicUsize = AtomicUsize::new(0);
/// Endpoint cptr
pub(crate) static EP_CPTR: AtomicU64 = AtomicU64::new(0);

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
