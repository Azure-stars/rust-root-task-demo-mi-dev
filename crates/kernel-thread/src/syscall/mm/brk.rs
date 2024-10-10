use common::{USPACE_HEAP_BASE, USPACE_HEAP_SIZE};
use syscalls::Errno;

use crate::{syscall::SysResult, task::Sel4Task};

pub(crate) fn sys_brk(task: &mut Sel4Task, addr: *mut u8) -> SysResult {
    let addr = addr as usize;
    if addr < USPACE_HEAP_BASE || addr > USPACE_HEAP_BASE + USPACE_HEAP_SIZE {
        return Err(Errno::ENOMEM);
    }
    if addr > task.heap {
        task.brk(addr);
    }
    task.heap = addr;
    Ok(0)
}
