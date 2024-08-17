use core::cmp;

use alloc::collections::btree_map::BTreeMap;
use sel4::{debug_println, BootInfo, CapRights, SmallPage, VMAttributes, VSpace};
use xmas_elf::{program, ElfFile};

use crate::{child::map_page_to_vspace, object_allocator::ObjectAllocator, page_seat_vaddr};

const PAGE_SIZE: usize = 4096;
pub const DEFAULT_USER_STACK_SIZE: usize = 0x1_0000_0000;

/// Create a new Monolithic Task from the given elf file.
pub fn map_elf(obj_allocator: &mut ObjectAllocator, vspace: VSpace, elf_data: &[u8]) {
    let file = ElfFile::new(elf_data).expect("This is not a valid elf file");

    let mut mapped_page: BTreeMap<usize, SmallPage> = BTreeMap::new();

    // Load data from elf file.
    file.program_iter()
        .filter(|ph| ph.get_type() == Ok(program::Type::Load))
        .for_each(|ph| {
            let mut offset = ph.offset() as usize;
            let mut vaddr = ph.virtual_addr() as usize;
            let end = offset + ph.file_size() as usize;
            let vaddr_end = vaddr + ph.mem_size() as usize;

            loop {
                if vaddr >= vaddr_end {
                    break;
                }

                let page_cap = match mapped_page.remove(&(vaddr / PAGE_SIZE * PAGE_SIZE)) {
                    Some(page_cap) => {
                        page_cap.frame_unmap().unwrap();
                        page_cap
                    }
                    None => obj_allocator.allocate_page(),
                };

                // If need to read data from elf file.
                if offset < end {
                    // Map to root task to write datas.
                    page_cap
                        .frame_map(
                            BootInfo::init_thread_vspace(),
                            page_seat_vaddr(),
                            CapRights::all(),
                            VMAttributes::DEFAULT,
                        )
                        .unwrap();

                    let rsize = cmp::min(PAGE_SIZE - vaddr % PAGE_SIZE, end - offset);
                    // Copy data from elf file's data to the correct position.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            elf_data.as_ptr().add(offset),
                            (page_seat_vaddr() + offset % PAGE_SIZE) as *mut u8,
                            rsize,
                        )
                    }

                    unsafe {
                        if vaddr < 0x1fc40 && vaddr + PAGE_SIZE > 0x1fc40 {
                            let v = ((page_seat_vaddr() + 0xc40) as *mut u32).read_volatile();
                            debug_println!("v: {:#x}", v);
                        }
                    }

                    page_cap.frame_unmap().unwrap();

                    offset += rsize;
                }

                map_page_to_vspace(
                    obj_allocator,
                    vspace,
                    vaddr / PAGE_SIZE * PAGE_SIZE,
                    page_cap,
                );

                mapped_page.insert(vaddr / PAGE_SIZE * PAGE_SIZE, page_cap);

                // Calculate offset
                vaddr += PAGE_SIZE - vaddr % PAGE_SIZE;
            }
        });
}

/// Map user stack
pub fn map_stack(
    obj_allocator: &mut ObjectAllocator,
    vspace: VSpace,
    mut start: usize,
    end: usize,
) {
    start = start / PAGE_SIZE * PAGE_SIZE;
    for vaddr in (start..end).step_by(PAGE_SIZE) {
        let page_cap = obj_allocator.allocate_page();
        map_page_to_vspace(obj_allocator, vspace, vaddr, page_cap);
    }
}
