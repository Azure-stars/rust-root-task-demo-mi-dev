use sel4::{cap_type, BootInfo, CapRights, LocalCPtr, VMAttributes};
use sel4_sys::seL4_DebugPutChar;
use syscalls::{Errno, Sysno};

use crate::{object_allocator::alloc_cap, page_seat_vaddr, task::Sel4Task, utils::align_bits};

pub fn handle_ipc_call(
    task: &mut Sel4Task,
    sys_id: usize,
    args: [usize; 6],
) -> Result<usize, Errno> {
    let sys_no = Sysno::new(sys_id).ok_or(Errno::EINVAL)?;
    // debug_println!("received sys_no: {:?}", sys_no);
    log::debug!("received sys_no: {:?}", sys_no);
    let res = match sys_no {
        Sysno::set_tid_address => 1,
        Sysno::getuid => 0,
        Sysno::brk => task.brk(args[0]),
        Sysno::mmap => {
            let addr = match args[0] {
                0 => 0x3_0000_0000,
                _ => args[0],
            };
            for vaddr in (addr..addr + args[1]).step_by(0x1000) {
                if task.mapped_page.get(&vaddr).is_some() {
                    continue;
                }
                let page_cap = alloc_cap::<cap_type::Granule>();
                task.map_page(vaddr, page_cap);
            }
            addr
        }
        Sysno::write => match args[0] {
            1 => {
                if let Some(cap) = task.mapped_page.get(&align_bits(args[1], 12)) {
                    let new_cap = LocalCPtr::<sel4::cap_type::SmallPage>::from_bits(0);
                    BootInfo::init_thread_cnode()
                        .relative(new_cap)
                        .copy(
                            &BootInfo::init_thread_cnode().relative(*cap),
                            CapRights::all(),
                        )
                        .unwrap();

                    new_cap
                        .frame_map(
                            BootInfo::init_thread_vspace(),
                            page_seat_vaddr(),
                            CapRights::all(),
                            VMAttributes::DEFAULT,
                        )
                        .unwrap();

                    let bytes = unsafe {
                        core::slice::from_raw_parts(page_seat_vaddr() as *const u8, 0x1000)
                    };

                    // FIXME: ensure that data in the page.z
                    bytes[args[1] % 0x1000..args[1] % 0x1000 + args[2]]
                        .iter()
                        .map(u8::clone)
                        .for_each(seL4_DebugPutChar);
                    new_cap.frame_unmap().unwrap();

                    BootInfo::init_thread_cnode()
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
