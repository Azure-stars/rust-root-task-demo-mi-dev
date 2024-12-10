use crate::{syscall::SysResult, task::Sel4Task};

pub(crate) fn sys_exit(task: &mut Sel4Task, exit_code: i32) -> SysResult {
    task.exit = Some(exit_code);
    task.tcb.tcb_suspend().unwrap();
    Ok(0)
}

pub(crate) fn sys_exit_group(task: &mut Sel4Task, exit_code: i32) -> SysResult {
    task.exit = Some(exit_code);
    task.tcb.tcb_suspend().unwrap();
    Ok(0)
}
