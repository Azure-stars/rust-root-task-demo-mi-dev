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
extern crate sel4_panicking;

mod alloc_impl;
mod child;
mod ipc_call;
mod irq_test;
mod object_allocator;
mod task;
mod utils;
use core::fmt;

use log::{Level, Record};
use object_allocator::{allocate_ep, OBJ_ALLOCATOR};
use sel4::{debug_println, set_ipc_buffer, IPCBuffer, LocalCPtr};
use sel4_logging::{LevelFilter, Logger};
use sel4_sys::seL4_DebugPutChar;

sel4_panicking_env::register_debug_put_char!(seL4_DebugPutChar);

/// Get the virtual address of the page seat.
pub fn page_seat_vaddr() -> usize {
    0x1_0000_2000
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

#[export_name = "_start"]
fn main(ipc_buffer: IPCBuffer) -> sel4::Result<!> {
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

    set_ipc_buffer(ipc_buffer);

    // sel4::debug_snapshot();

    // TODO: Init ObjAllocator
    OBJ_ALLOCATOR.lock().init(19..1023, LocalCPtr::from_bits(17 as _));

    test_func!("Test IRQ", irq_test::test_irq());

    test_func!("Test Thread", {
        let ep = allocate_ep();
        child::test_child(ep).unwrap()
    });

    sel4::BootInfo::init_thread_tcb().tcb_suspend()?;
    unreachable!()
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    debug_println!("Task Error");
    loop {}
}
