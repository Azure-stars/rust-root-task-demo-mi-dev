#![no_std]
#![no_main]

mod vsyscall;

pub use vsyscall::vsyscall_handler;
