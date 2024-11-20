use crate_consts::{PAGE_SIZE, PAGE_SIZE_BITS};
use sel4::{init_thread, Cap, CapRights, VmAttributes};
use sel4_sys::seL4_DebugPutChar;
use syscalls::{Errno, Sysno};

use crate::{object_allocator::OBJ_ALLOCATOR, page_seat_vaddr, task::Sel4Task, utils::align_bits};

pub fn handle_ipc_call(
    task: &mut Sel4Task,
    sys_id: usize,
    args: [usize; 6],
) -> Result<usize, Errno> {
    let sys_no = Sysno::new(sys_id).ok_or(Errno::EINVAL)?;
    log::debug!("received sys_no: {:?}", sys_no);
    let res = match sys_no {
        Sysno::set_tid_address => 1,
        Sysno::getuid => 0,
        Sysno::brk => task.brk(args[0]),
        Sysno::mmap => {
            // TODO: checking if the addr is aligned to page size
            let addr = match args[0] {
                0 => 0x3_0000_0000,
                _ => args[0],
            };
            let len = args[1];

            for vaddr in (addr..addr + len).step_by(PAGE_SIZE) {
                if task.mapped_page.get(&vaddr).is_some() {
                    continue;
                }
                let page_cap = OBJ_ALLOCATOR
                    .lock()
                    .allocate_fixed_sized::<sel4::cap_type::Granule>();
                task.map_page(vaddr, page_cap);
            }
            addr
        }
        Sysno::write => match args[0] {
            1 => {
                if let Some(cap) = task.mapped_page.get(&align_bits(args[1], PAGE_SIZE_BITS)) {
                    let new_cap = Cap::<sel4::cap_type::SmallPage>::from_bits(0);
                    init_thread::slot::CNODE
                        .cap()
                        .relative(new_cap)
                        .copy(
                            &init_thread::slot::CNODE.cap().relative(*cap),
                            CapRights::all(),
                        )
                        .unwrap();

                    new_cap
                        .frame_map(
                            init_thread::slot::VSPACE.cap(),
                            page_seat_vaddr(),
                            CapRights::all(),
                            VmAttributes::DEFAULT,
                        )
                        .unwrap();

                    let bytes = unsafe {
                        core::slice::from_raw_parts(page_seat_vaddr() as *const u8, PAGE_SIZE)
                    };

                    // FIXME: ensure that data in the page
                    bytes[args[1] % PAGE_SIZE..args[1] % PAGE_SIZE + args[2]]
                        .iter()
                        .map(u8::clone)
                        .for_each(seL4_DebugPutChar);
                    new_cap.frame_unmap().unwrap();

                    init_thread::slot::CNODE
                        .cap()
                        .relative(new_cap)
                        .delete()
                        .unwrap();
                }
                args[2]
            }
            _ => unimplemented!("Write to {} is unimplemented", args[0]),
        },
        Sysno::munmap => 0,
        Sysno::exit_group => {
            task.exit = true;
            0
        }
        Sysno::mprotect => Err(Errno::EPERM)?,
        _ => {
            panic!("sysno: {:?}", sys_no)
        }
    };
    Ok(res)
}
