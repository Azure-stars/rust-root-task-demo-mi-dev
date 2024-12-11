extern crate alloc;
use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use core::ops::Range;
use crate_consts::GRANULE_SIZE;
use object::{
    elf::{PF_R, PF_W, PF_X},
    Object, ObjectSegment, SegmentFlags,
};

use crate::ObjectAllocator;
// 计算 elf image 的虚地址空间范围
pub fn footprint<'a>(image: &'a impl Object<'a>) -> Range<usize> {
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

// 将ELF的虚地址空间 map 到页表中，但不分配物理页
pub fn map_intermediate_translation_tables(
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
                .allocate_and_retype(ty.blueprint())
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

/// 将 ELF image 映射到物理页
pub fn map_image<'a>(
    allocator: &mut ObjectAllocator,
    mapped_page: &mut BTreeMap<usize, sel4::cap::Granule>,
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
                allocator.allocate_and_retyped_fixed_sized::<sel4::cap_type::Granule>(),
                sel4::CapRightsBuilder::all(),
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
                    sel4::CapRights::all(),
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
        mapped_page.insert(addr, page_cap);
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