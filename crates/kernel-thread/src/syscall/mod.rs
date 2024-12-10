use sel4::{cap::Endpoint, debug_println};
use syscalls::{Errno, Sysno};
mod fs;
mod mm;
mod net;
mod thread;
use crate::task::Sel4Task;

type SysResult = Result<usize, Errno>;

pub fn handle_ipc_call(
    task: &mut Sel4Task,
    sys_id: usize,
    args: [usize; 6],
    fault_ep: Endpoint,
) -> Result<usize, Errno> {
    let sys_no = Sysno::new(sys_id).ok_or(Errno::EINVAL)?;
    debug_println!("[KernelThread] Syscall: {:?}", sys_no);
    match sys_no {
        Sysno::write => fs::sys_write(task, args[0] as _, args[1] as _, args[2] as _),
        Sysno::brk => mm::sys_brk(task, args[0] as _),
        Sysno::mmap => mm::sys_mmap(
            task,
            args[0] as _,
            args[1] as _,
            args[2] as _,
            args[3] as _,
            args[4] as _,
            args[5] as _,
        ),
        Sysno::munmap => mm::sys_unmap(task, args[0] as _, args[1] as _),
        Sysno::exit => thread::sys_exit(task, args[0] as _),
        Sysno::exit_group => thread::sys_exit_group(task, args[0] as _),
        Sysno::getpid => thread::sys_getpid(task),
        Sysno::execve => thread::sys_exec(task, fault_ep, args[0] as _, args[1] as _, args[2] as _),

        Sysno::getppid => thread::sys_getppid(task),
        Sysno::set_tid_address => thread::sys_set_tid_address(task, args[0] as _),
        Sysno::getuid => thread::sys_getuid(task),
        Sysno::geteuid => thread::sys_geteuid(task),

        Sysno::socket => net::sys_socket(task, args[0] as _, args[1] as _, args[2] as _),
        Sysno::accept => net::sys_accept(task, args[0] as _, args[1] as _, args[2] as _),
        Sysno::bind => net::sys_bind(task, args[0] as _, args[1] as _, args[2] as _),
        Sysno::connect => net::sys_connect(task, args[0] as _, args[1] as _, args[2] as _),
        Sysno::listen => net::sys_listen(task, args[0] as _),
        Sysno::sendto => net::sys_sendto(
            task,
            args[0] as _,
            args[1] as _,
            args[2] as _,
            args[3] as _,
            args[4] as _,
            args[5] as _,
        ),
        Sysno::recvfrom => net::sys_recvfrom(
            task,
            args[0] as _,
            args[1] as _,
            args[2] as _,
            args[3] as _,
            args[4] as _,
            args[5] as _,
        ),
        Sysno::shutdown => net::sys_shutdown(task, args[0] as _, args[1] as _),
        _ => Err(Errno::ENOSYS),
    }
}
