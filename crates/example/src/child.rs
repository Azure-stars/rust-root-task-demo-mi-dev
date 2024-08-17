use core::cmp;

use common::CustomMessageLabel;
use sel4::{
    debug_println, r#yield, with_ipc_buffer, BootInfo, CNodeCapData, CapRights, Endpoint, Error,
    Fault, Result, SmallPage, VMAttributes, VSpace,
};
use xmas_elf::ElfFile;

use crate::{
    elf::{map_elf, map_stack, DEFAULT_USER_STACK_SIZE},
    object_allocator::ObjectAllocator,
    page_seat_vaddr,
};

// TODO: Make elf file path dynamically available.
const CHILD_ELF: &[u8] = include_bytes!("../../../build/shim-comp.elf");
const BUSYBOX_ELF: &[u8] = include_bytes!("../../../busybox");

pub fn test_child(obj_allocator: &mut ObjectAllocator, ep: Endpoint) -> Result<()> {
    // let ipc_buffer_cap = LocalCPtr::<sel4::cap_type::SmallPage>::from_bits(
    //     UserImageUtils.get_user_image_frame_slot(ipc_buffer.ptr() as usize) as u64,
    // );
    let tcb = obj_allocator.allocate_tcb();

    let cnode = obj_allocator.allocate_cnode(12);

    // Copy tcb to target cnode
    cnode
        .relative_bits_with_depth(1, 12)
        .copy(
            &BootInfo::init_thread_cnode().relative(tcb),
            CapRights::all(),
        )
        .unwrap();
    // Copy EndPoint to target cnode
    cnode
        .relative_bits_with_depth(ep.cptr().bits(), 12)
        .copy(
            &BootInfo::init_thread_cnode().relative(ep),
            CapRights::all(),
        )
        .unwrap();

    let vspace = obj_allocator.allocate_vspace();

    BootInfo::init_thread_asid_pool()
        .asid_pool_assign(vspace)
        .unwrap();

    map_elf(obj_allocator, vspace, CHILD_ELF);

    map_stack(
        obj_allocator,
        vspace,
        DEFAULT_USER_STACK_SIZE - 0x10000,
        DEFAULT_USER_STACK_SIZE,
    );

    let file = ElfFile::new(CHILD_ELF).expect("can't load elf file");
    let ipc_buffer_cap = obj_allocator.allocate_page();
    let max = file
        .section_iter()
        .fold(0, |acc, x| cmp::max(acc, x.address() + x.size()));
    let ipc_buffer_addr = (max + 4096 - 1) / 4096 * 4096;
    map_page_to_vspace(obj_allocator, vspace, ipc_buffer_addr as _, ipc_buffer_cap);

    tcb.tcb_configure(
        ep.cptr(),
        cnode,
        CNodeCapData::new(0, sel4::WORD_SIZE - 12),
        vspace,
        ipc_buffer_addr,
        ipc_buffer_cap,
    )?;
    tcb.tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 0, 255)?;

    // let tp = [0u8; 0x200];
    let mut user_context = sel4::UserContext::default();

    // *user_context.pc_mut() = child_task as u64;
    *user_context.pc_mut() = file.header.pt2.entry_point();
    *user_context.sp_mut() = DEFAULT_USER_STACK_SIZE as _;
    // user_context.inner_mut().tpidr_el0 = tp.as_ptr() as u64;
    // user_context.inner_mut().tpidr_el0 = ipc_buffer_addr + 0x1000;
    if let Some(tls) = file.find_section_by_name(".tbss") {
        user_context.inner_mut().tpidr_el0 = tls.address();
    }
    *user_context.gpr_mut(0) = ep.cptr().bits();
    *user_context.gpr_mut(1) = ipc_buffer_addr;

    tcb.tcb_write_all_registers(false, &mut user_context)
        .unwrap();

    tcb.tcb_set_affinity(0).unwrap();
    tcb.debug_name(b"before name");

    // sel4::debug_snapshot();
    tcb.tcb_resume().unwrap();

    loop {
        // sys_null(-10);
        r#yield();
        // sys_null(-10);
        let (message, _badge) = ep.recv(());

        if message.label() < 0x8 {
            let fault = with_ipc_buffer(|buffer| Fault::new(&buffer, &message));
            debug_println!("fault: {:#x?}", fault);
            match fault {
                Fault::VMFault(vmfault) => {
                    // if !vmfault.is_prefetch() {
                    //     break;
                    // }
                    let vaddr = vmfault.addr() as usize / 4096 * 4096;
                    let page_cap = obj_allocator.allocate_page();

                    if vmfault.addr() != 0x12345678 {
                        // Map to root task to write datas.
                        page_cap
                            .frame_map(
                                BootInfo::init_thread_vspace(),
                                page_seat_vaddr(),
                                CapRights::all(),
                                VMAttributes::DEFAULT,
                            )
                            .unwrap();

                        // Copy data to page
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                vaddr as *mut u8,
                                page_seat_vaddr() as *mut u8,
                                4096,
                            )
                        }

                        page_cap.frame_unmap().unwrap();
                    }

                    // Move cap to child's cnode and map for child vspace.

                    // cnode
                    //     .relative_bits_with_depth(0x100, 12)
                    //     .mutate(&BootInfo::init_thread_cnode().relative(page_cap), 0)
                    //     .unwrap();

                    map_page_to_vspace(obj_allocator, vspace, vaddr, page_cap);

                    tcb.tcb_resume().unwrap();
                }
                _ => {}
            }
        } else {
            match CustomMessageLabel::try_from(&message) {
                Some(CustomMessageLabel::TestCustomMessage) => {
                    debug_println!("recv {} length message: ", message.length());
                    with_ipc_buffer(|buffer| {
                        buffer.msg_bytes()[..message.length()]
                            .iter()
                            .enumerate()
                            .for_each(|(i, x)| {
                                sel4::debug_print!("{:#x} ", *x);
                                if i % 16 == 15 {
                                    debug_println!();
                                }
                            });
                    });
                }
                Some(CustomMessageLabel::Exit) => {
                    break;
                }
                None => {
                    debug_println!("recv {} length message {:#x?} ", message.length(), message);
                }
            }
        }
    }

    tcb.tcb_suspend().unwrap();

    // TODO: Free memory from slots.
    let rcnode = BootInfo::init_thread_cnode();

    rcnode.relative(cnode).revoke().unwrap();
    rcnode.relative(cnode).delete().unwrap();

    rcnode.relative(tcb).revoke().unwrap();
    rcnode.relative(tcb).delete().unwrap();

    rcnode.relative(vspace).revoke().unwrap();
    rcnode.relative(vspace).delete().unwrap();

    rcnode.relative(ep).revoke().unwrap();
    rcnode.relative(ep).delete().unwrap();
    Ok(())
}

pub fn map_page_to_vspace(
    obj_allocator: &mut ObjectAllocator,
    vspace: VSpace,
    vaddr: usize,
    page_cap: SmallPage,
) {
    for _ in 0..4 {
        let res: core::result::Result<(), sel4::Error> =
            page_cap.frame_map(vspace, vaddr as _, CapRights::all(), VMAttributes::DEFAULT);
        match res {
            Ok(_) => return,
            Err(Error::FailedLookup) => {
                let pt_cap = obj_allocator.allocate_pt();
                pt_cap.pt_map(vspace, vaddr, VMAttributes::DEFAULT).unwrap();
            }
            _ => res.unwrap(),
        }
    }
}
