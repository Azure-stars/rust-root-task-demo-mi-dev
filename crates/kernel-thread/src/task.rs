use core::{
    cmp,
    sync::atomic::{AtomicU64, Ordering},
};

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use common::{USPACE_BASE, USPACE_HEAP_BASE};
use sel4::{cap_type, BootInfo, CapRights, Error, SmallPage, VMAttributes};
use xmas_elf::{program, ElfFile};

use crate::{
    object_allocator::{alloc_cap, alloc_cap_size},
    page_seat_vaddr,
};

const PAGE_SIZE: usize = 0x1000;
const STACK_ALIGN_SIZE: usize = 16;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[allow(non_camel_case_types, dead_code)]
pub enum AuxV {
    /// end of vector
    NULL = 0,
    /// entry should be ignored
    IGNORE = 1,
    /// file descriptor of program
    EXECFD = 2,
    /// program headers for program
    PHDR = 3,
    /// size of program header entry
    PHENT = 4,
    /// number of program headers
    PHNUM = 5,
    /// system page size
    PAGESZ = 6,
    /// base address of interpreter
    BASE = 7,
    /// flags
    FLAGS = 8,
    /// entry point of program
    ENTRY = 9,
    /// program is not ELF
    NOTELF = 10,
    /// real uid
    UID = 11,
    /// effective uid
    EUID = 12,
    /// real gid
    GID = 13,
    /// effective gid
    EGID = 14,
    /// string identifying CPU for optimizations
    PLATFORM = 15,
    /// arch dependent hints at CPU capabilities
    HWCAP = 16,
    /// frequency at which times() increments
    CLKTCK = 17,
    // values 18 through 22 are reserved
    DCACHEBSIZE = 19,
    /// secure mode boolean
    SECURE = 23,
    /// string identifying real platform, may differ from AT_PLATFORM
    BASE_PLATFORM = 24,
    /// address of 16 random bytes
    RANDOM = 25,
    /// extension of AT_HWCAP
    HWCAP2 = 26,
    /// filename of program
    EXECFN = 31,
}

pub struct Sel4Task {
    pub tcb: sel4::TCB,
    pub cnode: sel4::CNode,
    pub vspace: sel4::VSpace,
    pub mapped_pt: Vec<sel4::PT>,
    pub mapped_page: BTreeMap<usize, sel4::SmallPage>,
    pub heap: usize,
    pub exit: Option<i32>,
    pub id: u64,
    pub pid: u64,
    /// The clear thread tid field
    ///
    /// See <https://manpages.debian.org/unstable/manpages-dev/set_tid_address.2.en.html#clear_child_tid>
    ///
    /// When the thread exits, the kernel clears the word at this address if it is not NULL.
    pub clear_child_tid: Option<usize>,
}

impl Drop for Sel4Task {
    fn drop(&mut self) {
        let root_cnode = BootInfo::init_thread_cnode();
        root_cnode.relative(self.tcb).revoke().unwrap();
        root_cnode.relative(self.tcb).delete().unwrap();
        root_cnode.relative(self.cnode).revoke().unwrap();
        root_cnode.relative(self.cnode).delete().unwrap();
        root_cnode.relative(self.vspace).revoke().unwrap();
        root_cnode.relative(self.vspace).delete().unwrap();

        self.mapped_pt.iter().for_each(|cap| {
            root_cnode.relative(*cap).revoke().unwrap();
            root_cnode.relative(*cap).delete().unwrap();
        });
        self.mapped_page.values().for_each(|cap| {
            root_cnode.relative(*cap).revoke().unwrap();
            root_cnode.relative(*cap).delete().unwrap();
        });
    }
}

impl Sel4Task {
    pub fn new() -> Sel4Task {
        static ID_COUNTER: AtomicU64 = AtomicU64::new(1);
        let vspace = alloc_cap::<cap_type::VSpace>();

        BootInfo::init_thread_asid_pool()
            .asid_pool_assign(vspace)
            .unwrap();

        Sel4Task {
            tcb: alloc_cap::<cap_type::TCB>(),
            cnode: alloc_cap_size::<cap_type::CNode>(12),
            vspace,
            mapped_pt: Vec::new(),
            mapped_page: BTreeMap::new(),
            heap: USPACE_HEAP_BASE,
            exit: None,
            id: ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            pid: 0,
            clear_child_tid: None,
        }
    }

    /// To find a free area in the vspace.
    ///
    /// The area starts from `start` and the size is `size`.
    pub fn find_free_area(&self, start: usize, size: usize) -> Option<usize> {
        let mut last_addr = USPACE_BASE.max(start);
        for (vaddr, _page) in &self.mapped_page {
            if last_addr + size <= *vaddr {
                return Some(last_addr);
            }
            last_addr = *vaddr + PAGE_SIZE;
        }
        // TODO: Set the limit of the top of the user space.
        Some(last_addr)
    }

    pub fn map_page(&mut self, vaddr: usize, page: sel4::SmallPage, rights: CapRights) {
        assert_eq!(vaddr % PAGE_SIZE, 0);
        for _ in 0..4 {
            let res: core::result::Result<(), sel4::Error> = page.frame_map(
                self.vspace,
                vaddr as _,
                rights.clone(),
                VMAttributes::DEFAULT,
            );
            match res {
                Ok(_) => {
                    self.mapped_page.insert(vaddr, page);
                    return;
                }
                Err(Error::FailedLookup) => {
                    let pt_cap = alloc_cap::<cap_type::PT>();
                    pt_cap
                        .pt_map(self.vspace, vaddr, VMAttributes::DEFAULT)
                        .unwrap();
                    self.mapped_pt.push(pt_cap);
                }
                _ => res.unwrap(),
            }
        }
    }

    pub fn unmap_page(&mut self, vaddr: usize, page: sel4::SmallPage) {
        assert_eq!(vaddr % PAGE_SIZE, 0);
        let res = page.frame_unmap();
        match res {
            Ok(_) => {
                self.mapped_page.remove(&vaddr);
            }
            _ => res.unwrap(),
        }
    }

    pub fn map_stack(
        &mut self,
        file: &ElfFile,
        mut start: usize,
        end: usize,
        args: &[&str],
    ) -> usize {
        start = start / PAGE_SIZE * PAGE_SIZE;
        let mut stack_ptr = end;

        for vaddr in (start..end).step_by(PAGE_SIZE) {
            let page_cap = alloc_cap::<cap_type::Granule>();
            if vaddr == end - PAGE_SIZE {
                // The last page is used to store the arguments.
                page_cap
                    .frame_map(
                        BootInfo::init_thread_vspace(),
                        page_seat_vaddr(),
                        CapRights::all(),
                        VMAttributes::DEFAULT,
                    )
                    .unwrap();

                let args_ptr: Vec<_> = args
                    .iter()
                    .map(|arg| {
                        // TODO: set end bit was zeroed manually.
                        stack_ptr = (stack_ptr - arg.bytes().len() - 1) / STACK_ALIGN_SIZE
                            * STACK_ALIGN_SIZE;
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                arg.as_bytes().as_ptr(),
                                (page_seat_vaddr() + stack_ptr % PAGE_SIZE) as *mut u8,
                                arg.bytes().len(),
                            );
                        }
                        stack_ptr
                    })
                    .collect();

                let mut push_num = |num: usize| {
                    stack_ptr = stack_ptr - core::mem::size_of::<usize>();

                    unsafe {
                        ((page_seat_vaddr() + stack_ptr % PAGE_SIZE) as *mut usize)
                            .write_volatile(num);
                    }

                    stack_ptr
                };

                let mut auxv = BTreeMap::new();
                auxv.insert(AuxV::EXECFN, args_ptr[0]);
                auxv.insert(AuxV::PAGESZ, PAGE_SIZE);
                auxv.insert(AuxV::ENTRY, file.header.pt2.entry_point() as usize);
                auxv.insert(AuxV::GID, 0);
                auxv.insert(AuxV::EGID, 0);
                auxv.insert(AuxV::UID, 0);
                auxv.insert(AuxV::EUID, 0);
                auxv.insert(AuxV::NULL, 0);

                // push auxiliary vector
                auxv.into_iter().for_each(|(key, v)| {
                    push_num(v);
                    push_num(key as usize);
                });
                // push environment
                push_num(0);
                // push args pointer
                push_num(0);
                args_ptr.iter().rev().for_each(|x| {
                    push_num(*x);
                });
                // push argv
                push_num(args_ptr.len());

                // Unmap Frame
                page_cap.frame_unmap().unwrap();
            }
            self.map_page(vaddr, page_cap, CapRights::all());
        }
        stack_ptr
    }

    pub fn map_elf(&mut self, elf_data: &[u8]) {
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

                while vaddr < vaddr_end {
                    let page_cap = match mapped_page.remove(&(vaddr / PAGE_SIZE * PAGE_SIZE)) {
                        Some(page_cap) => {
                            page_cap.frame_unmap().unwrap();
                            page_cap
                        }
                        None => alloc_cap::<cap_type::Granule>(),
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

                        page_cap.frame_unmap().unwrap();

                        offset += rsize;
                    }

                    self.map_page(vaddr / PAGE_SIZE * PAGE_SIZE, page_cap, CapRights::all());

                    mapped_page.insert(vaddr / PAGE_SIZE * PAGE_SIZE, page_cap);

                    // Calculate offset
                    vaddr += PAGE_SIZE - vaddr % PAGE_SIZE;
                }
            });
    }

    pub fn brk(&mut self, value: usize) {
        for vaddr in (self.heap..value).step_by(0x1000) {
            let page_cap = alloc_cap::<cap_type::Granule>();
            self.map_page(vaddr, page_cap, CapRights::all());
        }
    }
}
