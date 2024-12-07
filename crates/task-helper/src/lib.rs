#![no_std]
#![feature(associated_type_defaults)]

extern crate alloc;

use alloc::{collections::btree_map::BTreeMap, sync::Arc, vec::Vec};
use core::marker::PhantomData;
use crate_consts::{
    DEFAULT_THREAD_FAULT_EP, DEFAULT_THREAD_IRQ_EP, DEFAULT_THREAD_NOTIFICATION, PAGE_SIZE,
    STACK_ALIGN_SIZE,
};
use sel4::{
    cap::{Granule, Notification, Null},
    init_thread, AbsoluteCPtr, CNodeCapData, CPtr, CPtrBits, CapRights, Error, HasCPtrWithDepth,
    VmAttributes as VMAttributes,
};
use sel4_sync::{lock_api::Mutex, MutexSyncOpsWithNotification};
use xmas_elf::{program, ElfFile};

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

/// The Trait the help task implement quickly.
pub trait TaskHelperTrait<V> {
    type Task = V;
    /// The default stack top address.
    const DEFAULT_STACK_TOP: usize;
    /// Allocate a new page table.
    fn allocate_pt(task: &mut V) -> sel4::cap::PT;
    /// Allocate a new Page.
    fn allocate_page(task: &mut V) -> sel4::cap::Granule;
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
        mapped_page: BTreeMap<usize, sel4::cap::Granule>,
        badge: u64,
        irq_ep: sel4::cap::Endpoint,
    ) -> Self {
        let task = Self {
            tcb,
            cnode,
            vspace,
            mapped_pt: Arc::new(Mutex::new(Vec::new())),
            mapped_page,
            stack_bottom: H::DEFAULT_STACK_TOP,
            phantom: PhantomData,
        };

        // Move Fault EP to child process
        task.abs_cptr(DEFAULT_THREAD_FAULT_EP)
            .mint(&cnode_relative(fault_ep), CapRights::all(), badge)
            .unwrap();

        // Move IRQ EP to child process
        task.abs_cptr(DEFAULT_THREAD_IRQ_EP)
            .mint(&cnode_relative(irq_ep), CapRights::all(), badge)
            .unwrap();

        // Copy ASIDPool to the task, children can assign another children.
        task.abs_cptr(init_thread::slot::ASID_POOL.cptr_bits())
            .copy(
                &cnode_relative(init_thread::slot::ASID_POOL.cap()),
                CapRights::all(),
            )
            .unwrap();

        // Copy ASIDControl to the task, children can assign another children.
        task.abs_cptr(init_thread::slot::ASID_CONTROL.cptr_bits())
            .copy(
                &cnode_relative(init_thread::slot::ASID_CONTROL.cap()),
                CapRights::all(),
            )
            .unwrap();

        task
    }

    /// Map a [sel4::Granule] to vaddr.
    pub fn map_page(&mut self, vaddr: usize, page: sel4::cap::Granule) {
        assert_eq!(vaddr % PAGE_SIZE, 0);
        for _ in 0..sel4::vspace_levels::NUM_LEVELS {
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
        unreachable!("Failed to map page!")
    }

    /// Configure task with setting CNode, Tcb and VSpace Cap
    pub fn configure(
        &mut self,
        radix_bits: usize,
        ipc_buffer_addr: usize,
        ipc_buffer_cap: Granule,
    ) -> Result<(), Error> {
        // Move cap rights to task
        self.abs_cptr(init_thread::slot::CNODE.cptr_bits())
            .mint(
                &cnode_relative(self.cnode),
                CapRights::all(),
                CNodeCapData::skip_high_bits(radix_bits).into_word(),
            )
            .unwrap();

        // Copy tcb to task
        self.abs_cptr(init_thread::slot::TCB.cptr_bits())
            .copy(&cnode_relative(self.tcb), CapRights::all())
            .unwrap();

        // Copy vspace to task
        self.abs_cptr(init_thread::slot::VSPACE.cptr_bits())
            .copy(&cnode_relative(self.vspace), CapRights::all())
            .unwrap();

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
    pub fn abs_cptr(&self, cptr_bits: CPtrBits) -> AbsoluteCPtr {
        self.cnode
            .relative(Null::from_cptr(CPtr::from_bits(cptr_bits)))
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
pub fn cnode_relative<T: HasCPtrWithDepth>(path: T) -> AbsoluteCPtr {
    init_thread::slot::CNODE.cap().relative(path)
}
