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
mod image_utils;
mod ipc_call;
mod irq_test;
mod object_allocator;
mod task;
mod utils;
use core::{cell::UnsafeCell, fmt};

use image_utils::UserImageUtils;
use log::{Level, Record};
use object_allocator::{allocate_ep, OBJ_ALLOCATOR};
use sel4::{cap_type::SmallPage, debug_println, LocalCPtr, GRANULE_SIZE};
use sel4_logging::{LevelFilter, Logger};
use sel4_root_task::root_task;

#[repr(align(4096))]
pub struct AlignedPage(UnsafeCell<[u8; 4096]>);

static mut PAGE_FRAME_SEAT: AlignedPage = AlignedPage(UnsafeCell::new([0; GRANULE_SIZE.bytes()]));

/// Get the virtual address of the page seat.
pub fn page_seat_vaddr() -> usize {
    unsafe { PAGE_FRAME_SEAT.0.get() as _ }
}

pub fn fmt_with_module(record: &Record, f: &mut fmt::Formatter) -> fmt::Result {
    let target = match record.target().is_empty() {
        true => record.module_path().unwrap_or_default(),
        false => record.target(),
    };
    let color_code = match record.level() {
        Level::Error => 31u8, // Red
        Level::Warn => 93,    // BrightYellow
        Level::Info => 34,    // Blue
        Level::Debug => 32,   // Green
        Level::Trace => 90,   // BrightBlack
    };

    write!(
        f,
        "\u{1B}[{}m\
            [{}] [{}] {}\
            \u{1B}[0m",
        color_code,
        record.level(),
        target,
        record.args()
    )
}

pub fn level_to_filter(log_level: Option<&str>) -> LevelFilter {
    match log_level {
        Some("error") => LevelFilter::Error,
        Some("warn") => LevelFilter::Warn,
        Some("info") => LevelFilter::Info,
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        _ => LevelFilter::Debug,
    }
}

#[root_task]
fn main(bootinfo: &sel4::BootInfo) -> sel4::Result<!> {
    static mut LOGGER: Logger = sel4_logging::LoggerBuilder::const_default()
        .write(|s| sel4::debug_print!("{}", s))
        .fmt(fmt_with_module)
        .build();

    unsafe {
        LOGGER.level_filter = match option_env!("LOG") {
            Some("error") => LevelFilter::Error,
            Some("warn") => LevelFilter::Warn,
            Some("info") => LevelFilter::Info,
            Some("debug") => LevelFilter::Debug,
            Some("trace") => LevelFilter::Trace,
            _ => LevelFilter::Debug,
        };
        LOGGER.set().unwrap();
        debug_println!();
        debug_println!("[Kernel Thread] Log Filter: {:?}", LOGGER.level_filter());
    }

    UserImageUtils.init(bootinfo);

    let page_seat_cap = UserImageUtils.get_user_image_frame_slot(page_seat_vaddr()) as _;
    LocalCPtr::<SmallPage>::from_bits(page_seat_cap)
        .frame_unmap()
        .unwrap();

    OBJ_ALLOCATOR.lock().init(bootinfo);

    test_func!("Test IRQ", irq_test::test_irq());

    test_func!("Test Thread", {
        let ep = allocate_ep();
        child::test_child(ep).unwrap()
    });

    sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    unreachable!()
}
