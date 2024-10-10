use common::{STDERR_FD, STDOUT_FD};
use memory_addr::{MemoryAddr, VirtAddr, PAGE_SIZE_4K};
use sel4::{BootInfo, CapRights, LocalCPtr, VMAttributes};
use sel4_panicking_env::debug_println;
use sel4_sys::seL4_DebugPutChar;
use syscalls::Errno;

use crate::{page_seat_vaddr, syscall::SysResult, task::Sel4Task, utils::align_bits};

pub(crate) fn sys_write(
    task: &mut Sel4Task,
    fd: i32,
    buf: *const u8,
    mut count: usize,
) -> SysResult {
    if fd != STDOUT_FD && fd != STDERR_FD {
        return Err(Errno::ENOSYS);
    }
    let mut buf_addr = VirtAddr::from_ptr_of(buf);
    debug_println!("buf_addr: {:?} count: {}", buf_addr, count);
    let mut payload_length = 0;
    while count > 0 {
        if let Some(cap) = task.mapped_page.get(&align_bits(buf_addr.as_usize(), 12)) {
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
            let copy_len = (PAGE_SIZE_4K - buf_addr.align_offset_4k()).min(count);
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    page_seat_vaddr() as *const u8,
                    copy_len + buf_addr.align_offset_4k(),
                )
            };

            // FIXME: ensure that data in the page.z
            bytes[buf_addr.align_offset_4k()..(buf_addr.align_offset_4k() + copy_len)]
                .iter()
                .map(u8::clone)
                .for_each(seL4_DebugPutChar);

            count -= copy_len;
            buf_addr += copy_len;
            payload_length += copy_len;
            new_cap.frame_unmap().unwrap();
            BootInfo::init_thread_cnode()
                .relative(new_cap)
                .delete()
                .unwrap();
        }
    }
    Ok(payload_length)
}
