#![no_std]
#![feature(associated_type_defaults)]

use core::{cmp, marker::PhantomData};

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use sel4::{
    cap_type, sys, AbsoluteCPtr, BootInfo, CNodeCapData, CPtr, CPtrBits, CapRights, Error, Granule,
    HasCPtrWithDepth, LocalCPtr, VMAttributes,
};
use xmas_elf::{program, ElfFile};

extern crate alloc;

/// The size of the page [sel4::GRANULE_SIZE].
const PAGE_SIZE: usize = sel4::GRANULE_SIZE.bytes();

/// Stack aligned with [STACK_ALIGN_SIZE] bytes
const STACK_ALIGN_SIZE: usize = 16;

/// The Trait the help task implement quickly.
pub trait TaskHelperTrait<V> {
    type Task = V;
    /// The address of IPC buffer.
    const IPC_BUFFER_ADDR: usize;
    /// The default stack top address.
    const DEFAULT_STACK_TOP: usize;
    /// Get the address of the empty seat page.
    fn page_seat_vaddr() -> usize;
    /// Allocate a new page table.
    fn allocate_pt(task: &mut V) -> sel4::PT;
    /// Allocate a new Page.
    fn allocate_page(task: &mut V) -> sel4::Granule;
}

/// The macro help to align addr with [PAGE_SIZE].
macro_rules! align_page {
    ($addr:expr) => {
        ($addr / PAGE_SIZE * PAGE_SIZE)
    };
}

/// Help to create a new task quickly.
pub struct Sel4TaskHelper<H: TaskHelperTrait<Self>> {
    pub tcb: sel4::TCB,
    pub cnode: sel4::CNode,
    pub vspace: sel4::VSpace,
    pub mapped_pt: Vec<sel4::PT>,
    pub mapped_page: BTreeMap<usize, sel4::Granule>,
    pub stack_bottom: usize,
    pub phantom: PhantomData<H>,
}

impl<H: TaskHelperTrait<Self>> Sel4TaskHelper<H> {
    pub fn new(
        tcb: sel4::TCB,
        cnode: sel4::CNode,
        fault_ep: sel4::Endpoint,
        vspace: sel4::VSpace,
    ) -> Self {
        let task = Self {
            tcb,
            cnode,
            vspace,
            mapped_pt: Vec::new(),
            mapped_page: BTreeMap::new(),
            stack_bottom: H::DEFAULT_STACK_TOP,
            phantom: PhantomData,
        };

        // Move Fault EP to child process
        task.abs_cptr(18 as _)
            .mint(&init_abs_cptr(fault_ep), CapRights::all(), 1)
            .unwrap();

        // Copy ASIDPool to the task, children can assign another children.
        task.abs_cptr(sys::seL4_RootCapSlot::seL4_CapInitThreadASIDPool.into())
            .copy(
                &init_abs_cptr(BootInfo::init_thread_asid_pool()),
                CapRights::all(),
            )
            .unwrap();

        // Copy ASIDControl to the task, children can assign another children.
        task.abs_cptr(sys::seL4_RootCapSlot::seL4_CapASIDControl.into())
            .copy(&init_abs_cptr(BootInfo::asid_control()), CapRights::all())
            .unwrap();

        task
    }
    /// Map a [sel4::Granule] to vaddr.
    pub fn map_page(&mut self, vaddr: usize, page: sel4::Granule) {
        assert_eq!(vaddr % PAGE_SIZE, 0);
        for _ in 0..4 {
            let res: core::result::Result<(), sel4::Error> = page.frame_map(
                self.vspace,
                vaddr as _,
                CapRights::all(),
                VMAttributes::DEFAULT,
            );
            match res {
                Ok(_) => {
                    self.mapped_page.insert(vaddr, page);
                    return;
                }
                // Map page tbale if the fault is Error::FailedLookup
                // (It's indicates that here was not a page table).
                Err(Error::FailedLookup) => {
                    let pt_cap = H::allocate_pt(self);
                    pt_cap
                        .pt_map(self.vspace, vaddr, VMAttributes::DEFAULT)
                        .unwrap();
                    self.mapped_pt.push(pt_cap);
                }
                _ => res.unwrap(),
            }
        }
    }

    /// Map a elf file to the current [sel4::VSpace].
    pub fn map_elf(&mut self, file: ElfFile) {
        // Load data from elf file.
        file.program_iter()
            .filter(|ph| ph.get_type() == Ok(program::Type::Load))
            .for_each(|ph| {
                let mut offset = ph.offset() as usize;
                let mut vaddr = ph.virtual_addr() as usize;
                let end = offset + ph.file_size() as usize;
                let vaddr_end = vaddr + ph.mem_size() as usize;

                while vaddr < vaddr_end {
                    // Create or get the capability for the current address.
                    let page_cap = match self.mapped_page.remove(&align_page!(vaddr)) {
                        Some(page_cap) => {
                            page_cap.frame_unmap().unwrap();
                            page_cap
                        }
                        None => H::allocate_page(self),
                    };
                    // If need to read data from elf file.
                    if offset < end {
                        // Map to root task to write datas.
                        page_cap
                            .frame_map(
                                BootInfo::init_thread_vspace(),
                                H::page_seat_vaddr(),
                                CapRights::all(),
                                VMAttributes::DEFAULT,
                            )
                            .unwrap();

                        let rsize = cmp::min(PAGE_SIZE - vaddr % PAGE_SIZE, end - offset);
                        // Copy data from elf file's data to the correct position.
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                file.input.as_ptr().add(offset),
                                (H::page_seat_vaddr() + offset % PAGE_SIZE) as *mut u8,
                                rsize,
                            )
                        }
                        // Unmap frame for make seat empty.
                        page_cap.frame_unmap().unwrap();

                        offset += rsize;
                    }

                    // Map the page to current task's virtual address space.
                    self.map_page(align_page!(vaddr), page_cap);

                    // Calculate offset
                    vaddr += PAGE_SIZE - vaddr % PAGE_SIZE;
                }
            });

        // Set current task's context.
        let mut user_context = sel4::UserContext::default();
        // Set entry point address.
        *user_context.pc_mut() = file.header.pt2.entry_point();
        // Set stack top address.
        *user_context.sp_mut() = (H::DEFAULT_STACK_TOP - STACK_ALIGN_SIZE) as _;
        *user_context.gpr_mut(0) = H::IPC_BUFFER_ADDR as _;
        // Set TLS base address.
        user_context.inner_mut().tpidr_el0 = file
            .program_iter()
            .find(|x| x.get_type() == Ok(program::Type::Tls))
            .map(|x| x.virtual_addr())
            .unwrap_or(0);
        // Write register to the task.
        self.tcb
            .tcb_write_all_registers(false, &mut user_context)
            .expect("can't write pc reg to tcb");
    }

    /// Initialize the IPC buffer and return the address and the capability of the buffer.
    pub fn init_ipc_buffer(&mut self) -> (usize, Granule) {
        let addr = H::IPC_BUFFER_ADDR;
        assert_eq!(
            addr % PAGE_SIZE,
            0,
            "ipc buffer address should aligned with {PAGE_SIZE:#x}"
        );

        let cap = match self.mapped_page.get(&addr) {
            Some(cap) => cap.clone(),
            None => {
                let page = H::allocate_page(self);
                self.map_page(addr, page);
                self.mapped_page.insert(addr, page);
                page
            }
        };
        (addr, cap)
    }

    /// Configure the [sel4::TCB] in the task
    pub fn configure(&mut self, radix_bits: usize) -> Result<(), Error> {
        let (ib_addr, ib_cap) = self.init_ipc_buffer();

        // Move cap rights to child process
        self.abs_cptr(sys::seL4_RootCapSlot::seL4_CapInitThreadCNode.into())
            .mint(
                &init_abs_cptr(self.cnode),
                CapRights::all(),
                CNodeCapData::skip_high_bits(radix_bits).into_word(),
            )
            .unwrap();

        // Copy tcb to task's Cap Space.
        self.abs_cptr(sys::seL4_RootCapSlot::seL4_CapInitThreadTCB.into())
            .copy(&init_abs_cptr(self.tcb), CapRights::all())
            .unwrap();

        self.abs_cptr(sys::seL4_RootCapSlot::seL4_CapInitThreadVSpace.into())
            .copy(&init_abs_cptr(self.vspace), CapRights::all())
            .unwrap();

        // Configure the tcb structure
        self.tcb.tcb_configure(
            // TODO: make this in a constant
            CPtr::from_bits(18),
            self.cnode,
            CNodeCapData::skip_high_bits(radix_bits),
            self.vspace,
            ib_addr as _,
            ib_cap,
        )
    }

    /// Map specified count pages to the stack bottom.
    pub fn map_stack(&mut self, page_count: usize) {
        self.stack_bottom -= page_count * PAGE_SIZE;
        for i in 0..page_count {
            let page_cap = H::allocate_page(self);
            self.map_page(self.stack_bottom + i * PAGE_SIZE, page_cap);
        }
    }

    /// Get the the absolute cptr related to task's cnode through cptr_bits.
    pub fn abs_cptr(&self, cptr_bits: CPtrBits) -> AbsoluteCPtr {
        self.cnode
            .relative(LocalCPtr::<cap_type::Null>::from_cptr(CPtr::from_bits(
                cptr_bits,
            )))
    }
}

/// Get the the absolute cptr related to current cnode through cptr_bits.
pub fn init_abs_cptr<T: HasCPtrWithDepth>(path: T) -> AbsoluteCPtr {
    BootInfo::init_thread_cnode().relative(path)
}
