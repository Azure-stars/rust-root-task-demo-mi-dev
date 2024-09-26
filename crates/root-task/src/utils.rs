use sel4::{init_thread, AbsoluteCPtr, BootInfo, HasCPtrWithDepth};

/// Send a syscall to sel4 with none arguments
#[allow(dead_code)]
pub fn sys_null(sys: isize) {
    unsafe {
        core::arch::asm!(
            "svc 0",
            in("x7") sys,
        );
    }
}

/// Get [AbsoluteCPtr] from current CSpace though path.
pub fn abs_cptr<T: HasCPtrWithDepth>(path: T) -> AbsoluteCPtr {
    init_thread::slot::CNODE.cap().relative(path)
}
