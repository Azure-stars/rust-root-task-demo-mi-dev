//
// Copyright 2023, Colias Group, LLC
//
// SPDX-License-Identifier: BSD-2-Clause
//

#![no_std]
#![no_main]
#![feature(never_type)]
#![feature(const_trait_impl)]
#![feature(effects)]

extern crate alloc;

mod alloc_impl;
mod child;
mod elf;
mod image_utils;
mod irq_test;
mod object_allocator;
mod tls;
mod utils;
use core::cell::UnsafeCell;

use image_utils::UserImageUtils;
use sel4::{cap_type::SmallPage, LocalCPtr, GRANULE_SIZE};
use sel4_logging::{LevelFilter, Logger};
use sel4_root_task::root_task;

#[repr(align(4096))]
pub struct AlignedPage(UnsafeCell<[u8; 4096]>);

static mut PAGE_FRAME_SEAT: AlignedPage = AlignedPage(UnsafeCell::new([0; GRANULE_SIZE.bytes()]));

/// Get the virtual address of the page seat.
pub fn page_seat_vaddr() -> usize {
    unsafe { PAGE_FRAME_SEAT.0.get() as _ }
}

#[root_task]
fn main(bootinfo: &sel4::BootInfo) -> sel4::Result<!> {
    static LOGGER: Logger = sel4_logging::LoggerBuilder::const_default()
        .level_filter(LevelFilter::Debug)
        .write(|s| sel4::debug_print!("{}", s))
        .build();
    LOGGER.set().unwrap();

    sel4::debug_println!("log filter: {:?}", LOGGER.level_filter());
    UserImageUtils.init(bootinfo);

    LocalCPtr::<SmallPage>::from_bits(
        UserImageUtils.get_user_image_frame_slot(unsafe { PAGE_FRAME_SEAT.0.get() as _ }) as _,
    )
    .frame_unmap()
    .unwrap();

    let mut obj_allocator = object_allocator::ObjectAllocator::new(bootinfo);

    test_func!("Test IRQ", irq_test::test_irq(&mut obj_allocator));

    test_func!("Test Thread", {
        let ep = obj_allocator.allocate_ep();
        child::test_child(&mut obj_allocator, ep).unwrap()
    });

    sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    unreachable!()
}
