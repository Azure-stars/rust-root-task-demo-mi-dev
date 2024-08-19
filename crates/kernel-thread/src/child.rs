use core::cmp;

use common::CustomMessageLabel;
use sel4::{
    debug_println, r#yield, reply, with_ipc_buffer, with_ipc_buffer_mut, BootInfo, CNodeCapData,
    CapRights, Endpoint, Fault, MessageInfo, Result, Word,
};
use xmas_elf::ElfFile;

use crate::{
    ipc_call::handle_ipc_call,
    object_allocator::allocate_page,
    task::{Sel4Task, DEFAULT_USER_STACK_SIZE},
    utils::align_bits,
};

// TODO: Make elf file path dynamically available.
const CHILD_ELF: &[u8] = include_bytes!("../../../build/shim-comp.elf");
const BUSYBOX_ELF: &[u8] = include_bytes!("../../../busybox");

pub fn test_child(ep: Endpoint) -> Result<()> {
    let args = &["busybox", "echo", "BusyboxRunEcho"];
    log::debug!("Run Command {:?}", args);
    let mut task = Sel4Task::new();

    // sel4::debug_snapshot();

    // Copy tcb to target cnode
    task.cnode
        .relative_bits_with_depth(1, 12)
        .copy(
            &BootInfo::init_thread_cnode().relative_bits_with_depth(task.tcb.bits(), 12),
            CapRights::all(),
        )
        .unwrap();

    // Copy EndPoint to target cnode
    task.cnode
        .relative_bits_with_depth(ep.cptr().bits(), 12)
        .copy(
            &BootInfo::init_thread_cnode().relative_bits_with_depth(ep.bits(), 12),
            CapRights::all(),
        )
        .unwrap();

    task.map_elf(CHILD_ELF);
    task.map_elf(BUSYBOX_ELF);
    let busybox_file = ElfFile::new(BUSYBOX_ELF).expect("can't load busybox file");
    let busybox_root = busybox_file.header.pt2.entry_point();

    let sp_ptr = task.map_stack(
        &busybox_file,
        DEFAULT_USER_STACK_SIZE - 0x10000,
        DEFAULT_USER_STACK_SIZE,
        args
    );

    let file = ElfFile::new(CHILD_ELF).expect("can't load elf file");
    let ipc_buffer_cap = allocate_page();
    let max = file
        .section_iter()
        .fold(0, |acc, x| cmp::max(acc, x.address() + x.size()));
    let ipc_buffer_addr = (max + 4096 - 1) / 4096 * 4096;
    task.map_page(ipc_buffer_addr as _, ipc_buffer_cap);

    // Configure the child task
    task.tcb.tcb_configure(
        ep.cptr(),
        task.cnode,
        CNodeCapData::new(0, sel4::WORD_SIZE - 12),
        task.vspace,
        ipc_buffer_addr,
        ipc_buffer_cap,
    )?;
    task.tcb
        .tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 0, 255)?;

    let mut user_context = sel4::UserContext::default();

    // Set child task's context
    *user_context.pc_mut() = file.header.pt2.entry_point();
    *user_context.sp_mut() = sp_ptr as _;
    *user_context.gpr_mut(0) = ep.cptr().bits();
    *user_context.gpr_mut(1) = ipc_buffer_addr;
    *user_context.gpr_mut(2) = busybox_root;
    // Write vsyscall root
    *user_context.gpr_mut(3) = busybox_file
        .find_section_by_name(".vsyscall")
        .map(|x| x.address())
        .unwrap_or(0);

    user_context.inner_mut().tpidr_el0 = file
        .find_section_by_name(".tbss")
        .map_or(0, |tls| tls.address());

    task.tcb
        .tcb_write_all_registers(false, &mut user_context)
        .unwrap();

    task.tcb.tcb_set_affinity(0).unwrap();
    task.tcb.debug_name(b"before name");

    // sel4::debug_snapshot();
    task.tcb.tcb_resume().unwrap();

    loop {
        if task.exit {
            break;
        }
        let (message, _badge) = ep.recv(());

        if message.label() < 0x8 {
            let fault = with_ipc_buffer(|buffer| Fault::new(&buffer, &message));
            debug_println!("[Kernel Thread] Received Fault: {:#x?}", fault);
            match fault {
                Fault::VMFault(vmfault) => {
                    let vaddr = align_bits(vmfault.addr() as usize, 12);
                    let page_cap = allocate_page();

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
                    let res = handle_ipc_call(&mut task, sys_id, args)
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

    task.tcb.tcb_suspend().unwrap();

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