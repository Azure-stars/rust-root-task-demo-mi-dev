#![no_std]
#![no_main]
#![feature(exclusive_range_pattern)]
#![feature(extract_if)]
#![feature(async_closure)]
#![feature(let_chains)]
#![feature(panic_info_message)]
#![feature(stdsimd)]

#[macro_use]
extern crate alloc;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate log;

#[macro_use]
mod logging;

mod epoll;
// mod modules;
mod panic;
mod socket;
// mod syscall;
mod tasks;
mod user;
use devices::{self};
use vfscore::OpenFlags;

use crate::tasks::FileItem;

/// The kernel entry
#[export_name = "_start"]
fn main() {
    allocator::init();

    logging::init(option_env!("LOG"));

    let str = include_str!("banner.txt");
    println!("{}", str);

    // TODO: Fix devices and filesystems
    // get devices and init
    // devices::regist_devices_irq();

    // initialize filesystem
    fs::init();
    {
        FileItem::fs_open("/var", OpenFlags::O_DIRECTORY)
            .expect("can't open /var")
            .mkdir("tmp")
            .expect("can't create tmp dir");
    }

    // init kernel threads and async executor
    tasks::init();

    // Run Tasks.
    log::info!("run tasks");
    tasks::run_tasks();

    println!("Task All Finished!");
}
