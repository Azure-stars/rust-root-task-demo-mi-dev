use sel4::{AbsoluteCPtr, BootInfo, HasCPtrWithDepth};


/// Send a syscall to sel4 with none arguments
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
    BootInfo::init_thread_cnode()
        .relative(path)
}
