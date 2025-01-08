#![no_std]
#![no_main]
#![feature(never_type)]
#![feature(const_trait_impl)]

extern crate alloc;

#[macro_use]
mod r#macro;
mod api;
mod dev;
mod fops;
mod fs;
mod mounts;
mod root;
mod runtime;

use axdriver_base::{BaseDriverOps, DeviceType};
use axdriver_block::ramdisk::RamDisk;
use sel4::debug_println;

sel4_panicking_env::register_debug_put_char!(sel4::sys::seL4_DebugPutChar);

fn main() -> ! {
    debug_println!("[FSThread] EntryPoint");

    let dev = RamDisk::default();
    debug_println!("[FSThread] Created ramdisk with size: {:?}", dev.size());

    debug_println!("[FSThread] use block device: {:?}", dev.device_name());

    root::init_rootfs(dev::Disk::new(dev));
    debug_println!("[FSThread] rootfs initialized");

    sel4::cap::Tcb::from_bits(1).tcb_suspend().unwrap();
    unreachable!()
}
