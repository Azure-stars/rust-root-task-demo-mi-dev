use core::{cmp, ops::DerefMut};

use common::{footprint, map_image, USPACE_STACK_SIZE, USPACE_STACK_TOP};
use object::File;
use sel4::{cap::Endpoint, cap_type, debug_println, init_thread, CNodeCapData};
use xmas_elf::ElfFile;

use crate::{syscall::SysResult, task::Sel4Task, utils::init_free_page_addr, OBJ_ALLOCATOR};

pub(crate) fn sys_getpid(task: &mut Sel4Task) -> SysResult {
    Ok(task.pid as usize)
}

pub(crate) fn sys_getppid(task: &mut Sel4Task) -> SysResult {
    Ok(task.pid as usize)
}

pub(crate) fn sys_getuid(task: &mut Sel4Task) -> SysResult {
    Ok(task.id as usize)
}

pub(crate) fn sys_geteuid(task: &mut Sel4Task) -> SysResult {
    Ok(task.id as usize)
}

pub(crate) fn sys_set_tid_address(task: &mut Sel4Task, tidptr: *mut i32) -> SysResult {
    task.clear_child_tid = Some(tidptr as usize);
    Ok(task.id as usize)
}

bitflags::bitflags! {
    /// 用于 sys_clone 的选项
    #[derive(Debug, Clone, Copy)]
    pub struct CloneFlags: i32 {
        /// .
        const CLONE_NEWTIME = 1 << 7;
        /// Share the same VM  between processes
        const CLONE_VM = 1 << 8;
        /// Share the same fs info between processes
        const CLONE_FS = 1 << 9;
        /// Share open files between processes
        const CLONE_FILES = 1 << 10;
        /// Share signal handlers between processes
        const CLONE_SIGHAND = 1 << 11;
        /// Place a pidfd in the parent's pidfd
        const CLONE_PIDFD = 1 << 12;
        /// Continue tracing in the chil
        const CLONE_PTRACE = 1 << 13;
        /// Suspends the parent until the child wakes up
        const CLONE_VFORK = 1 << 14;
        /// Current process shares the same parent as the cloner
        const CLONE_PARENT = 1 << 15;
        /// Add to the same thread group
        const CLONE_THREAD = 1 << 16;
        /// Create a new namespace
        const CLONE_NEWNS = 1 << 17;
        /// Share SVID SEM_UNDO semantics
        const CLONE_SYSVSEM = 1 << 18;
        /// Set TLS info
        const CLONE_SETTLS = 1 << 19;
        /// Store TID in userlevel buffer in the parent before MM copy
        const CLONE_PARENT_SETTID = 1 << 20;
        /// Register exit futex and memory location to clear
        const CLONE_CHILD_CLEARTID = 1 << 21;
        /// Create clone detached
        const CLONE_DETACHED = 1 << 22;
        /// The tracing process can't force CLONE_PTRACE on this clone.
        const CLONE_UNTRACED = 1 << 23;
        /// Store TID in userlevel buffer in the child
        const CLONE_CHILD_SETTID = 1 << 24;
        /// New pid namespace.
        const CLONE_NEWPID = 1 << 29;
    }
}

const CHILD_ELF: &[u8] = include_bytes!("../../../../../build/shim.elf");
const BUSYBOX_ELF: &[u8] = include_bytes!("../../../../../busybox");

pub(crate) fn sys_exec(
    task: &mut Sel4Task,
    fault_ep: Endpoint,
    _path: *const u8,
    _argv: *const u8,
    _envp: *const u8,
) -> SysResult {
    let args = &["busybox", "--help"];

    let mut allocator = OBJ_ALLOCATOR.lock();
    let child_image = File::parse(CHILD_ELF).unwrap();
    task.mapped_page.clear();
    task.mapped_pt.clear();
    map_image(
        allocator.deref_mut(),
        &mut task.mapped_page,
        task.vspace,
        footprint(&child_image),
        &child_image,
        init_thread::slot::VSPACE.cap(),
        unsafe { init_free_page_addr() },
    );

    let busybox_img = File::parse(BUSYBOX_ELF).unwrap();
    map_image(
        allocator.deref_mut(),
        &mut task.mapped_page,
        task.vspace,
        footprint(&busybox_img),
        &busybox_img,
        init_thread::slot::VSPACE.cap(),
        unsafe { init_free_page_addr() },
    );
    drop(allocator);
    let busybox_file = ElfFile::new(BUSYBOX_ELF).expect("can't load busybox file");
    let busybox_root = busybox_file.header.pt2.entry_point();

    let sp_ptr = task.map_stack(
        &busybox_file,
        USPACE_STACK_TOP - USPACE_STACK_SIZE,
        USPACE_STACK_TOP,
        args,
    );

    let file = ElfFile::new(CHILD_ELF).expect("can't load elf file");
    let ipc_buffer_cap = OBJ_ALLOCATOR
        .lock()
        .allocate_and_retyped_fixed_sized::<cap_type::Granule>();
    let max = file
        .section_iter()
        .fold(0, |acc, x| cmp::max(acc, x.address() + x.size()));

    let ipc_buffer_addr = (max + 4096 - 1) / 4096 * 4096;

    task.map_page(ipc_buffer_addr as _, ipc_buffer_cap);

    // Configure the child task
    task.tcb
        .tcb_configure(
            fault_ep.cptr(),
            task.cnode,
            CNodeCapData::new(0, sel4::WORD_SIZE - 12),
            task.vspace,
            ipc_buffer_addr,
            ipc_buffer_cap,
        )
        .unwrap();

    task.tcb
        .tcb_set_sched_params(init_thread::slot::TCB.cap(), 0, 255)
        .unwrap();

    let mut user_context = sel4::UserContext::default();

    // Set child task's context
    *user_context.pc_mut() = file.header.pt2.entry_point();
    *user_context.sp_mut() = sp_ptr as _;
    *user_context.gpr_mut(1) = ipc_buffer_addr;
    *user_context.gpr_mut(2) = busybox_root;
    // Write vsyscall root
    *user_context.gpr_mut(3) = busybox_file
        .find_section_by_name(".vsyscall")
        .map(|x| x.address())
        .unwrap_or(0);
    // Get TSS section address.
    user_context.inner_mut().tpidr_el0 = file
        .find_section_by_name(".tbss")
        .map_or(0, |tls| tls.address());

    task.tcb
        .tcb_write_all_registers(false, &mut user_context)
        .unwrap();

    task.tcb.debug_name(b"before name");

    task.exit = Some(0);

    task.tcb.tcb_resume().unwrap();
    Ok(0)
}
