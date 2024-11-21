use crate::{ObjectAllocator, GRANULE_SIZE, OBJ_ALLOCATOR};
use alloc::vec::Vec;
use core::ops::{DerefMut, Range};
use crate_consts::CNODE_RADIX_BITS;
use object::{
    elf::{PF_R, PF_W, PF_X},
    File, Object, ObjectSegment, SegmentFlags,
};
use sel4::{
    cap::{Endpoint, Null},
    cap_type::{CNode, SmallPage, Tcb, PT},
    debug_println,
    init_thread::{self},
    CNodeCapData, CapRights,
};
use task_helper::{Sel4TaskHelper, TaskHelperTrait};
use xmas_elf::ElfFile;

pub struct TaskImpl;
pub type Sel4Task = Sel4TaskHelper<TaskImpl>;

impl TaskHelperTrait<Sel4TaskHelper<Self>> for TaskImpl {
    const DEFAULT_STACK_TOP: usize = 0x1_0000_0000;

    fn allocate_pt(_task: &mut Self::Task) -> sel4::cap::PT {
        OBJ_ALLOCATOR.lock().allocate_fixed_sized::<PT>()
    }

    fn allocate_page(_task: &mut Self::Task) -> sel4::cap::SmallPage {
        OBJ_ALLOCATOR.lock().allocate_fixed_sized::<SmallPage>()
    }
}

pub fn rebuild_cspace() {
    let cnode = OBJ_ALLOCATOR
        .lock()
        .allocate_variable_sized::<CNode>(CNODE_RADIX_BITS);
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
    irq_ep: Endpoint,
    thread_name: &str,
    file_data: &[u8],
    free_page_addr: usize,
) -> sel4::Result<Sel4Task> {
    // make 新线程的虚拟地址空间
    let (vspace, ipc_buffer_addr, ipc_buffer_cap) = make_child_vspace(
        &File::parse(file_data).unwrap(),
        sel4::init_thread::slot::VSPACE.cap(),
        free_page_addr,
        sel4::init_thread::slot::ASID_POOL.cap(),
    );

    let cnode = OBJ_ALLOCATOR
        .lock()
        .allocate_variable_sized::<CNode>(CNODE_RADIX_BITS);

    let tcb = OBJ_ALLOCATOR.lock().allocate_fixed_sized::<Tcb>();

    let mut task = Sel4Task::new(tcb, cnode, fault_ep.0, vspace, fault_ep.1, irq_ep);

    // Configure TCB
    task.configure(CNODE_RADIX_BITS, ipc_buffer_addr, ipc_buffer_cap)?;

    // Map stack for the task.
    task.map_stack(10);

    // set task priority and max control priority
    task.tcb
        .tcb_set_sched_params(init_thread::slot::TCB.cap(), 255, 255)?;

    task.tcb.debug_name(thread_name.as_bytes());

    task.with_context(&ElfFile::new(file_data).expect("parse elf error"));

    debug_println!(
        "[RootTask] Task: {} created. cnode: {:?}",
        thread_name,
        task.cnode
    );

    Ok(task)
}

pub fn run_tasks(tasks: &Vec<Sel4Task>) {
    tasks.iter().for_each(Sel4Task::run)
}

/// 创建一个新的虚拟地址空间
/// # Parameters
/// - `image`: ELF 文件
/// - `caller_vspace`: root-task 的虚拟地址空间
/// - `free_page_addr`: 空闲页的地址
/// - `asid_pool`: ASID 池
/// # Returns
/// - `sel4::cap::VSpace`: 新的虚拟地址空间
/// - `usize`: IPC buffer 的地址
/// - `sel4::cap::Granule`: IPC buffer 的 cap
pub(crate) fn make_child_vspace<'a>(
    image: &'a impl Object<'a>,
    caller_vspace: sel4::cap::VSpace,
    free_page_addr: usize,
    asid_pool: sel4::cap::AsidPool,
) -> (sel4::cap::VSpace, usize, sel4::cap::Granule) {
    let mut allocator = OBJ_ALLOCATOR.lock();
    let allocator = allocator.deref_mut();
    let child_vspace = allocator.allocate_fixed_sized::<sel4::cap_type::VSpace>();
    asid_pool.asid_pool_assign(child_vspace).unwrap();

    let image_footprint = footprint(image);

    // 将ELF的虚地址空间 map 到页表中，但不分配物理页
    map_intermediate_translation_tables(
        allocator,
        child_vspace,
        image_footprint.start..(image_footprint.end + GRANULE_SIZE),
    );

    // 将ELF的虚地址 map 到物理页
    map_image(
        allocator,
        child_vspace,
        image_footprint.clone(),
        image,
        caller_vspace,
        free_page_addr,
    );

    // make ipc buffer
    let ipc_buffer_addr = image_footprint.end;
    let ipc_buffer_cap = allocator.allocate_fixed_sized::<sel4::cap_type::Granule>();
    ipc_buffer_cap
        .frame_map(
            child_vspace,
            ipc_buffer_addr,
            sel4::CapRights::read_write(),
            sel4::VmAttributes::default(),
        )
        .unwrap();

    (child_vspace, ipc_buffer_addr, ipc_buffer_cap)
}

// 计算 elf image 的虚地址空间范围
fn footprint<'a>(image: &'a impl Object<'a>) -> Range<usize> {
    let min: usize = image
        .segments()
        .map(|seg| seg.address())
        .min()
        .unwrap()
        .try_into()
        .unwrap();
    let max: usize = image
        .segments()
        .map(|seg| seg.address() + seg.size())
        .max()
        .unwrap()
        .try_into()
        .unwrap();
    coarsen_footprint(&(min..max), GRANULE_SIZE)
}

fn map_intermediate_translation_tables(
    allocator: &mut ObjectAllocator,
    vspace: sel4::cap::VSpace,
    footprint: Range<usize>,
) {
    for level in 1..sel4::vspace_levels::NUM_LEVELS {
        let span_bytes = 1 << sel4::vspace_levels::span_bits(level);
        let footprint_at_level = coarsen_footprint(&footprint, span_bytes);
        for i in 0..(footprint_at_level.len() / span_bytes) {
            let ty = sel4::TranslationTableObjectType::from_level(level).unwrap();
            let addr = footprint_at_level.start + i * span_bytes;
            allocator
                .allocate(ty.blueprint())
                .cast::<sel4::cap_type::UnspecifiedIntermediateTranslationTable>()
                .generic_intermediate_translation_table_map(
                    ty,
                    vspace,
                    addr,
                    sel4::VmAttributes::default(),
                )
                .unwrap()
        }
    }
}

fn map_image<'a>(
    allocator: &mut ObjectAllocator,
    vspace: sel4::cap::VSpace,
    footprint: Range<usize>,
    image: &'a impl Object<'a>,
    caller_vspace: sel4::cap::VSpace,
    free_page_addr: usize,
) {
    // 计算需要的物理页数
    let num_pages = footprint.len() / GRANULE_SIZE;
    let mut pages = (0..num_pages)
        .map(|_| {
            (
                allocator.allocate_fixed_sized::<sel4::cap_type::Granule>(),
                sel4::CapRightsBuilder::none(),
            )
        })
        .collect::<Vec<(sel4::cap::Granule, sel4::CapRightsBuilder)>>();

    for seg in image.segments() {
        let segment_addr = usize::try_from(seg.address()).unwrap();
        let segment_size = usize::try_from(seg.size()).unwrap();
        let segment_footprint =
            coarsen_footprint(&(segment_addr..(segment_addr + segment_size)), GRANULE_SIZE);
        let num_pages_spanned_by_segment = segment_footprint.len() / GRANULE_SIZE;
        let segment_data_size = seg.data().unwrap().len();
        let segment_data_footprint = coarsen_footprint(
            &(segment_addr..(segment_addr + segment_data_size)),
            GRANULE_SIZE,
        );
        let num_pages_spanned_by_segment_data = segment_data_footprint.len() / GRANULE_SIZE;

        let segment_page_index_offset = (segment_footprint.start - footprint.start) / GRANULE_SIZE;

        for (_, rights) in &mut pages[segment_page_index_offset..][..num_pages_spanned_by_segment] {
            add_rights(rights, seg.flags());
        }

        let mut data = seg.data().unwrap();
        let mut offset_into_page = segment_addr % GRANULE_SIZE;
        for (page_cap, _) in
            &pages[segment_page_index_offset..][..num_pages_spanned_by_segment_data]
        {
            let data_len = (GRANULE_SIZE - offset_into_page).min(data.len());

            // 映射物理页到 root-task 的虚拟地址空间，并且将数据拷贝到物理页中
            page_cap
                .frame_map(
                    caller_vspace,
                    free_page_addr,
                    sel4::CapRights::read_write(),
                    sel4::VmAttributes::default(),
                )
                .unwrap();
            unsafe {
                ((free_page_addr + offset_into_page) as *mut u8).copy_from(data.as_ptr(), data_len);
            }
            page_cap.frame_unmap().unwrap();

            data = &data[data_len..];
            offset_into_page = 0;
        }
    }

    // 将物理页映射到 child 的虚拟地址空间
    for (i, (page_cap, rights)) in pages.into_iter().enumerate() {
        let addr = footprint.start + i * GRANULE_SIZE;
        page_cap
            .frame_map(vspace, addr, rights.build(), sel4::VmAttributes::default())
            .unwrap();
    }
}

fn add_rights(rights: &mut sel4::CapRightsBuilder, flags: SegmentFlags) {
    match flags {
        SegmentFlags::Elf { p_flags } => {
            if p_flags & PF_R != 0 {
                *rights = rights.read(true);
            }
            if p_flags & PF_W != 0 {
                *rights = rights.write(true);
            }
            if p_flags & PF_X != 0 {
                *rights = rights.grant(true);
            }
        }
        _ => unimplemented!(),
    }
}

fn coarsen_footprint(footprint: &Range<usize>, granularity: usize) -> Range<usize> {
    round_down(footprint.start, granularity)..footprint.end.next_multiple_of(granularity)
}

const fn round_down(n: usize, b: usize) -> usize {
    n - n % b
}
