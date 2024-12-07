use crate::{page_seat_vaddr, OBJ_ALLOCATOR};
use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use core::cmp;
use crate_consts::{CNODE_RADIX_BITS, PAGE_SIZE, STACK_ALIGN_SIZE};
use sel4::{
    cap_type::{CNode, Granule, Tcb, VSpace, PT},
    debug_println, init_thread, CapRights, Error, VmAttributes,
};
use xmas_elf::{program, ElfFile};

pub const DEFAULT_USER_STACK_SIZE: usize = 0x1_0000_0000;

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
    pub tcb: sel4::cap::Tcb,
    pub cnode: sel4::cap::CNode,
    pub vspace: sel4::cap::VSpace,
    pub mapped_pt: Vec<sel4::cap::PT>,
    pub mapped_page: BTreeMap<usize, sel4::cap::SmallPage>,
    pub heap: usize,
    pub exit: bool,
}

impl Drop for Sel4Task {
    fn drop(&mut self) {
        let root_cnode = init_thread::slot::CNODE.cap();
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
        let vspace = OBJ_ALLOCATOR
            .lock()
            .allocate_and_retyped_fixed_sized::<VSpace>();
        init_thread::slot::ASID_POOL
            .cap()
            .asid_pool_assign(vspace)
            .unwrap();

        let tcb = OBJ_ALLOCATOR
            .lock()
            .allocate_and_retyped_fixed_sized::<Tcb>();
        let cnode = OBJ_ALLOCATOR
            .lock()
            .allocate_and_retyped_variable_sized::<CNode>(CNODE_RADIX_BITS);

        Sel4Task {
            tcb,
            cnode,
            vspace,
            mapped_pt: Vec::new(),
            mapped_page: BTreeMap::new(),
            heap: 0x2_0000_0000,
            exit: false,
        }
    }

    pub fn map_page(&mut self, vaddr: usize, page: sel4::cap::SmallPage) {
        assert_eq!(vaddr % PAGE_SIZE, 0);
        for _ in 0..sel4::vspace_levels::NUM_LEVELS {
            let res: core::result::Result<(), sel4::Error> = page.frame_map(
                self.vspace,
                vaddr as _,
                CapRights::all(),
                VmAttributes::DEFAULT,
            );
            match res {
                Ok(_) => {
                    self.mapped_page.insert(vaddr, page);
                    return;
                }
                Err(Error::FailedLookup) => {
                    let pt_cap = OBJ_ALLOCATOR
                        .lock()
                        .allocate_and_retyped_fixed_sized::<PT>();
                    pt_cap
                        .pt_map(self.vspace, vaddr, VmAttributes::DEFAULT)
                        .unwrap();
                    self.mapped_pt.push(pt_cap);
                }
                _ => res.unwrap(),
            }
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
        let mut stack_ptr = DEFAULT_USER_STACK_SIZE;

        for vaddr in (start..end).step_by(PAGE_SIZE) {
            let page_cap = OBJ_ALLOCATOR
                .lock()
                .allocate_and_retyped_fixed_sized::<Granule>();
            if vaddr == DEFAULT_USER_STACK_SIZE - PAGE_SIZE {
                page_cap
                    .frame_map(
                        init_thread::slot::VSPACE.cap(),
                        page_seat_vaddr(),
                        CapRights::all(),
                        VmAttributes::DEFAULT,
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
            self.map_page(vaddr, page_cap);
        }
        stack_ptr
    }

    pub fn load_elf(&mut self, elf_data: &[u8]) {
        let file = ElfFile::new(elf_data).expect("This is not a valid elf file");

        let mut mapped_page: BTreeMap<usize, sel4::cap::SmallPage> = BTreeMap::new();

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
                        None => OBJ_ALLOCATOR
                            .lock()
                            .allocate_and_retyped_fixed_sized::<Granule>(),
                    };

                    // If need to read data from elf file.
                    if offset < end {
                        // Map to root task to write datas.
                        page_cap
                            .frame_map(
                                init_thread::slot::VSPACE.cap(),
                                page_seat_vaddr(),
                                CapRights::all(),
                                VmAttributes::DEFAULT,
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
                                debug_println!("[KernelThread] v: {:#x}", v);
                            }
                        }

                        page_cap.frame_unmap().unwrap();

                        offset += rsize;
                    }

                    self.map_page(vaddr / PAGE_SIZE * PAGE_SIZE, page_cap);

                    mapped_page.insert(vaddr / PAGE_SIZE * PAGE_SIZE, page_cap);

                    // Calculate offset
                    vaddr += PAGE_SIZE - vaddr % PAGE_SIZE;
                }
            });
    }

    pub fn brk(&mut self, value: usize) -> usize {
        if value == 0 {
            return self.heap;
        }
        for vaddr in (self.heap..value).step_by(PAGE_SIZE) {
            let page_cap = OBJ_ALLOCATOR
                .lock()
                .allocate_and_retyped_fixed_sized::<Granule>();
            self.map_page(vaddr, page_cap);
        }
        value
    }
}
