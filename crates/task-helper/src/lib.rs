#![no_std]
#![feature(associated_type_defaults)]

use alloc::{collections::btree_map::BTreeMap, sync::Arc, vec::Vec};
use core::{cmp, marker::PhantomData};
use crate_consts::{
    CNODE_RADIX_BITS, DEFAULT_THREAD_FAULT_EP, DEFAULT_THREAD_IRQ_EP, DEFAULT_THREAD_NOTIFICATION,
};
use object::{File, Object, ObjectSegment};
use sel4::{
    cap::{Granule, Notification, Null},
    init_thread, AbsoluteCPtr, CNodeCapData, CPtr, CPtrBits, CapRights, Error, HasCPtrWithDepth,
    VmAttributes as VMAttributes,
};
use sel4_sync::{lock_api::Mutex, MutexSyncOpsWithNotification};
use xmas_elf::{program, ElfFile};

extern crate alloc;

/// Thread Notifications implementation
pub struct ThreadNotification;

/// Implement [MutexSyncOpsWithNotification] for [ThreadNotification]
/// Get the notification in the specificed thread slot.
impl MutexSyncOpsWithNotification for ThreadNotification {
    fn notification(&self) -> Notification {
        Notification::from_bits(DEFAULT_THREAD_NOTIFICATION)
    }
}

/// Mutex with Notification.
// pub type NotiMutex<T> = Mutex<GenericRawMutex<ThreadNotification>, T>;
pub type NotiMutex<T> = Mutex<spin::Mutex<()>, T>;

/// The size of the page [sel4::GRANULE_SIZE].
const PAGE_SIZE: usize = sel4::FrameObjectType::GRANULE.bytes();

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
    fn allocate_pt(task: &mut V) -> sel4::cap::PT;
    /// Allocate a new Page.
    fn allocate_page(task: &mut V) -> sel4::cap::Granule;
}

/// The macro help to align addr with [PAGE_SIZE].
macro_rules! align_page {
    ($addr:expr) => {
        ($addr / PAGE_SIZE * PAGE_SIZE)
    };
}

/// Help to create a new task quickly.
pub struct Sel4TaskHelper<H: TaskHelperTrait<Self>> {
    pub tcb: sel4::cap::Tcb,
    pub cnode: sel4::cap::CNode,
    pub vspace: sel4::cap::VSpace,
    pub mapped_pt: Arc<NotiMutex<Vec<sel4::cap::PT>>>,
    pub mapped_page: BTreeMap<usize, sel4::cap::Granule>,
    pub stack_bottom: usize,
    pub phantom: PhantomData<H>,
}

impl<H: TaskHelperTrait<Self>> Sel4TaskHelper<H> {
    pub fn new(
        tcb: sel4::cap::Tcb,
        cnode: sel4::cap::CNode,
        fault_ep: sel4::cap::Endpoint,
        vspace: sel4::cap::VSpace,
        badge: u64,
        irq_ep: sel4::cap::Endpoint,
    ) -> Self {
        let task = Self {
            tcb,
            cnode,
            vspace,
            mapped_pt: Arc::new(Mutex::new(Vec::new())),
            mapped_page: BTreeMap::new(),
            stack_bottom: H::DEFAULT_STACK_TOP,
            phantom: PhantomData,
        };

        // Move Fault EP to child process
        task.abs_cptr_with_depth(DEFAULT_THREAD_FAULT_EP, CNODE_RADIX_BITS)
            .mint(&init_abs_cptr(fault_ep), CapRights::all(), badge)
            .unwrap();

        // Move IRQ EP to child process
        task.abs_cptr_with_depth(DEFAULT_THREAD_IRQ_EP, CNODE_RADIX_BITS)
            .mint(&init_abs_cptr(irq_ep), CapRights::all(), badge)
            .unwrap();

        // Copy ASIDPool to the task, children can assign another children.
        task.abs_cptr_with_depth(init_thread::slot::ASID_POOL.cptr_bits(), CNODE_RADIX_BITS)
            .copy(
                &init_abs_cptr(init_thread::slot::ASID_POOL.cap()),
                CapRights::all(),
            )
            .unwrap();

        // Copy ASIDControl to the task, children can assign another children.
        task.abs_cptr_with_depth(
            init_thread::slot::ASID_CONTROL.cptr_bits(),
            CNODE_RADIX_BITS,
        )
        .copy(
            &init_abs_cptr(init_thread::slot::ASID_CONTROL.cap()),
            CapRights::all(),
        )
        .unwrap();

        task
    }
    /// Map a [sel4::Granule] to vaddr.
    pub fn map_page(&mut self, vaddr: usize, page: sel4::cap::Granule) {
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
                    self.mapped_pt.lock().push(pt_cap);
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
                                init_thread::slot::VSPACE.cap(),
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
    pub fn configure(
        &mut self,
        radix_bits: usize,
        ipc_buffer_addr: usize,
        ipc_buffer_cap: Granule,
    ) -> Result<(), Error> {
        // Move cap rights to child process
        self.abs_cptr_with_depth(init_thread::slot::CNODE.cptr_bits(), CNODE_RADIX_BITS)
            .mint(
                &init_abs_cptr(self.cnode),
                CapRights::all(),
                CNodeCapData::skip_high_bits(radix_bits).into_word(),
            )
            .unwrap();

        // Copy tcb to task's Cap Space.
        self.abs_cptr_with_depth(init_thread::slot::TCB.cptr_bits(), CNODE_RADIX_BITS)
            .copy(&init_abs_cptr(self.tcb), CapRights::all())
            .unwrap();

        self.abs_cptr_with_depth(init_thread::slot::VSPACE.cptr_bits(), CNODE_RADIX_BITS)
            .copy(&init_abs_cptr(self.vspace), CapRights::all())
            .unwrap();

        // Configure the tcb structure
        self.tcb.tcb_configure(
            CPtr::from_bits(DEFAULT_THREAD_FAULT_EP),
            self.cnode,
            CNodeCapData::skip_high_bits(radix_bits),
            self.vspace,
            ipc_buffer_addr as _,
            ipc_buffer_cap,
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
    pub fn abs_cptr_with_depth(&self, cptr_bits: CPtrBits, depth: usize) -> AbsoluteCPtr {
        self.cnode.relative_bits_with_depth(cptr_bits, depth)
    }

    /// Clone a new thread from the current thread.
    pub fn clone_thread(&self, tcb: sel4::cap::Tcb) -> Self {
        Self {
            tcb,
            cnode: self.cnode,
            vspace: self.vspace,
            mapped_pt: self.mapped_pt.clone(),
            mapped_page: self.mapped_page.clone(),
            stack_bottom: self.stack_bottom,
            phantom: PhantomData,
        }
    }

    pub fn with_context(&self, image: &ElfFile) {
        let mut user_context = sel4::UserContext::default();
        *user_context.pc_mut() = image.header.pt2.entry_point();

        *user_context.sp_mut() = (H::DEFAULT_STACK_TOP - STACK_ALIGN_SIZE) as _;
        user_context.inner_mut().tpidr_el0 = image
            .program_iter()
            .find(|x| x.get_type() == Ok(program::Type::Tls))
            .map(|x| x.virtual_addr())
            .unwrap_or(0);

        self.tcb
            .tcb_write_all_registers(false, &mut user_context)
            .expect("can't write pc reg to tcb")
    }

    /// Run current task
    pub fn run(&self) {
        self.tcb.tcb_resume().unwrap();
    }
}

/// Get the the absolute cptr related to current cnode through cptr_bits.
pub fn init_abs_cptr<T: HasCPtrWithDepth>(path: T) -> AbsoluteCPtr {
    init_thread::slot::CNODE.cap().relative(path)
}
