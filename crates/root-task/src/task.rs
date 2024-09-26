use crate::obj_allocator::alloc_cap_size_slot;
use crate::utils::abs_cptr;
use crate::{
    obj_allocator::{alloc_cap, alloc_cap_size},
    page_seat_vaddr,
};
use alloc::vec::Vec;
use sel4::{
    cap::{Endpoint, Null},
    cap_type::CNode,
    init_thread::{self},
    CNodeCapData, CapRights,
};
use sel4::{cap_type, debug_println};
use task_helper::{Sel4TaskHelper, TaskHelperTrait};
use xmas_elf::ElfFile;

const CNODE_RADIX_BITS: usize = 12;

pub type Sel4Task = Sel4TaskHelper<TaskImpl>;

pub struct TaskImpl;

impl TaskHelperTrait<Sel4TaskHelper<Self>> for TaskImpl {
    const IPC_BUFFER_ADDR: usize = 0x1_0000_1000;
    const DEFAULT_STACK_TOP: usize = 0x1_0000_0000;
    fn page_seat_vaddr() -> usize {
        page_seat_vaddr()
    }

    fn allocate_pt(_task: &mut Self::Task) -> sel4::cap::PT {
        alloc_cap::<cap_type::PT>()
    }

    fn allocate_page(_task: &mut Self::Task) -> sel4::cap::SmallPage {
        alloc_cap::<cap_type::SmallPage>()
    }
}

pub fn rebuild_cspace() {
    let cnode = alloc_cap_size_slot::<CNode>(CNODE_RADIX_BITS);
    cnode
        .relative_bits_with_depth(0, CNODE_RADIX_BITS)
        .mint(
            &init_thread::slot::CNODE
                .cap()
                .relative(init_thread::slot::CNODE.cap()),
            CapRights::all(),
            CNodeCapData::skip(0).into_word(),
        )
        .unwrap();

    // load
    init_thread::slot::CNODE
        .cap()
        .relative(Null::from_bits(0))
        .mutate(
            &init_thread::slot::CNODE
                .cap()
                .relative(init_thread::slot::CNODE.cap()),
            CNodeCapData::skip_high_bits(CNODE_RADIX_BITS).into_word(),
        )
        .unwrap();

    sel4::cap::CNode::from_bits(0)
        .relative(init_thread::slot::CNODE.cap())
        .mint(
            &sel4::cap::CNode::from_bits(0).relative(cnode),
            CapRights::all(),
            CNodeCapData::skip_high_bits(CNODE_RADIX_BITS * 2).into_word(),
        )
        .unwrap();

    init_thread::slot::CNODE
        .cap()
        .relative(Null::from_bits(0))
        .delete()
        .unwrap();

    init_thread::slot::TCB
        .cap()
        .tcb_set_space(
            Null::from_bits(0).cptr(),
            cnode,
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS),
            init_thread::slot::VSPACE.cap(),
        )
        .unwrap();
}

pub fn build_kernel_thread(
    fault_ep: (Endpoint, u64),
    thread_name: &str,
    elf_file: ElfFile,
    irq_ep: Endpoint,
) -> sel4::Result<Sel4Task> {
    let cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let inner_cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let tcb = alloc_cap::<cap_type::Tcb>();
    let vspace = alloc_cap::<cap_type::VSpace>();

    // Build 2 level CSpace.
    // | unused (40 bits) | Level1 (12 bits) | Level0 (12 bits) |
    cnode
        .relative_bits_with_depth(0, CNODE_RADIX_BITS)
        .mutate(
            &abs_cptr(inner_cnode),
            CNodeCapData::skip(0).into_word() as _,
        )
        .unwrap();
    abs_cptr(Null::from_bits(0))
        .mutate(
            &abs_cptr(cnode),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word() as _,
        )
        .unwrap();
    abs_cptr(cnode)
        .mutate(
            &abs_cptr(Null::from_bits(0)),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word() as _,
        )
        .unwrap();

    init_thread::slot::ASID_POOL
        .cap()
        .asid_pool_assign(vspace)
        .unwrap();

    let mut task = Sel4Task::new(tcb, cnode, fault_ep.0, vspace, fault_ep.1, irq_ep);

    // Configure Root Task
    task.configure(2 * CNODE_RADIX_BITS)?;

    // Map stack for the task.
    task.map_stack(10);

    // set task priority and max control priority
    task.tcb
        .tcb_set_sched_params(init_thread::slot::TCB.cap(), 255, 255)?;

    // Map elf file for the task.
    task.map_elf(elf_file);

    task.tcb.debug_name(thread_name.as_bytes());

    debug_println!("Task: {} created. cnode: {:?}", thread_name, task.cnode);

    Ok(task)
}

pub fn run_tasks(tasks: &Vec<Sel4Task>) {
    tasks.iter().for_each(Sel4Task::run)
}
