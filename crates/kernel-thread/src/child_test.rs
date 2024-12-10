use crate::{
    syscall::handle_ipc_call,
    task::{Sel4Task, DEFAULT_USER_STACK_SIZE},
    utils::align_bits,
    OBJ_ALLOCATOR,
};
use alloc::collections::btree_map::BTreeMap;
use common::CustomMessageLabel;
use core::cmp;
use crate_consts::{CNODE_RADIX_BITS, DEFAULT_THREAD_FAULT_EP, PAGE_SIZE, PAGE_SIZE_BITS};
use sel4::{
    cap::Endpoint, cap_type::Granule, debug_println, init_thread, r#yield, reply, with_ipc_buffer,
    with_ipc_buffer_mut, CNodeCapData, CapRights, Fault, MessageInfo, Result, Word,
};
use spin::Mutex;
use xmas_elf::ElfFile;

// TODO: Make elf file path dynamically available.
const CHILD_ELF: &[u8] = include_bytes!("../../../build/test-thread.elf");
const BUSYBOX_ELF: &[u8] = include_bytes!("../../../busybox");

pub static TASK_MAP: Mutex<BTreeMap<u64, Sel4Task>> = Mutex::new(BTreeMap::new());

pub fn test_child(ep: Endpoint) -> Result<()> {
    let args = &["busybox", "echo", "Kernel Thread's Child Says Hello!"];
    debug_println!("[KernelThread] Child Task Start, busybox args: {:?}", args);
    let mut task = Sel4Task::new();

    // Copy tcb to child
    task.cnode
        .relative_bits_with_depth(1, CNODE_RADIX_BITS)
        .copy(
            &init_thread::slot::CNODE.cap().relative(task.tcb),
            CapRights::all(),
        )
        .unwrap();

    // Copy EndPoint to child
    task.cnode
        .relative_bits_with_depth(DEFAULT_THREAD_FAULT_EP, CNODE_RADIX_BITS)
        .mint(
            &init_thread::slot::CNODE.cap().relative(ep),
            CapRights::all(),
            TASK_MAP.lock().len() as u64,
        )?;

    debug_println!("[KernelThread] Child Task Mapping ELF...");
    task.load_elf(CHILD_ELF);
    task.load_elf(BUSYBOX_ELF);
    let child_elf_file = ElfFile::new(CHILD_ELF).expect("[KernelThread] can't load elf file");
    let busybox_file = ElfFile::new(BUSYBOX_ELF).expect("[KernelThread] can't load busybox file");

    let busybox_entry_point = busybox_file.header.pt2.entry_point();
    let sp_ptr = task.map_stack(
        &busybox_file,
        DEFAULT_USER_STACK_SIZE - 16 * PAGE_SIZE,
        DEFAULT_USER_STACK_SIZE,
        args,
    );

    let ipc_buffer_cap = OBJ_ALLOCATOR
        .lock()
        .allocate_and_retyped_fixed_sized::<Granule>();
    let max = child_elf_file
        .section_iter()
        .fold(0, |acc, x| cmp::max(acc, x.address() + x.size()));
    let ipc_buffer_addr = (max + 4096 - 1) / 4096 * 4096;
    task.map_page(ipc_buffer_addr as _, ipc_buffer_cap);

    // Configure the child task
    task.tcb.tcb_configure(
        ep.cptr(),
        task.cnode,
        CNodeCapData::new(0, sel4::WORD_SIZE - CNODE_RADIX_BITS),
        task.vspace,
        ipc_buffer_addr,
        ipc_buffer_cap,
    )?;
    task.tcb
        .tcb_set_sched_params(init_thread::slot::TCB.cap(), 0, 255)?;

    let mut user_context = sel4::UserContext::default();

    // Set child task's context
    *user_context.pc_mut() = child_elf_file.header.pt2.entry_point();
    *user_context.sp_mut() = sp_ptr as _;
    *user_context.gpr_mut(0) = ep.cptr().bits();
    *user_context.gpr_mut(1) = busybox_entry_point;
    // Write vsyscall section address to gpr2
    *user_context.gpr_mut(2) = busybox_file
        .find_section_by_name(".vsyscall")
        .map(|x| x.address())
        .unwrap_or(0);
    // Get TSS section address.
    user_context.inner_mut().tpidr_el0 = child_elf_file
        .find_section_by_name(".tbss")
        .map_or(0, |tls| tls.address());

    task.tcb
        .tcb_write_all_registers(false, &mut user_context)
        .unwrap();

    // task.tcb.tcb_set_affinity(0).unwrap();
    task.tcb.debug_name(b"before name");

    let mut task_map = TASK_MAP.lock();
    let task_badge = task_map.len();
    task.tcb.tcb_resume().unwrap();

    task_map.insert(task_badge as u64, task);
    loop {
        let (message, badge) = ep.recv(());

        let mut task = task_map.get_mut(&badge).unwrap();

        if message.label() < 8 {
            let fault = with_ipc_buffer(|buffer| Fault::new(&buffer, &message));
            debug_println!("[Kernel Thread] Received Fault: {:#x?}", fault);
            match fault {
                Fault::VmFault(vmfault) => {
                    let vaddr = align_bits(vmfault.addr() as usize, PAGE_SIZE_BITS);
                    let page_cap = OBJ_ALLOCATOR
                        .lock()
                        .allocate_and_retyped_fixed_sized::<Granule>();

                    task.map_page(vaddr, page_cap);

                    task.tcb.tcb_resume().unwrap();
                }
                _ => {}
            }
        } else {
            match CustomMessageLabel::try_from(&message) {
                Some(CustomMessageLabel::TestCustomMessage) => reply_with(&[]),
                Some(CustomMessageLabel::SysCall) => {
                    let (sys_id, args) = with_ipc_buffer(|ipc_buf| {
                        let msgs = ipc_buf.msg_regs();
                        let args: [Word; 6] = msgs[1..7].try_into().unwrap();
                        (msgs[0] as _, args.map(|x| x as usize))
                    });
                    let res = handle_ipc_call(&mut task, sys_id, args, ep)
                        .map_err(|e| -e.into_raw() as isize)
                        .unwrap_or_else(|e| e as usize);
                    reply_with(&[res]);
                }
                Some(CustomMessageLabel::Exit) => break,
                None => {
                    debug_println!(
                        "[Kernel Thread] Recv unknown {} length message {:#x?} ",
                        message.length(),
                        message
                    );
                }
            }
        }
        r#yield();
    }

    // TODO: Free memory from slots.
    Ok(())
}

/// Reply a message with empty message information
#[inline]
fn reply_with(regs: &[usize]) {
    with_ipc_buffer_mut(|buffer| {
        let msg_regs = buffer.msg_regs_mut();
        regs.iter()
            .enumerate()
            .for_each(|(i, reg)| msg_regs[i] = *reg as _);
        reply(buffer, MessageInfo::new(0, 0, 0, 8 * regs.len()))
    });
}
