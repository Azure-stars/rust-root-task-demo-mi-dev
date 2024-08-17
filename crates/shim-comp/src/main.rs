#![no_std]
#![no_main]
#![feature(naked_functions)]

use common::CustomMessageLabel;
use sel4::{
    debug_println, r#yield, set_ipc_buffer, with_ipc_buffer_mut, BootInfo, Endpoint, IPCBuffer,
    MessageInfo,
};

extern crate sel4_panicking;

sel4_panicking_env::register_debug_put_char!(sel4::debug_put_char);

#[no_mangle]
#[naked]
unsafe extern "C" fn _start() -> () {
    core::arch::asm!(
        "
            bl  {main}
            b   .
        ",
        main = sym main,
        options(noreturn)
    )
}

fn main(ep: Endpoint, ipc_buffer: IPCBuffer) {
    debug_println!("Hello Children: {:?}", ep);
    debug_println!("ipc buffer addr: {:p}", ipc_buffer.ptr());
    set_ipc_buffer(ipc_buffer);

    BootInfo::init_thread_tcb().debug_name(b"child task");

    // sys_null(-10);
    // sel4::debug_snapshot();

    with_ipc_buffer_mut(|buffer| {
        for i in 0..3 {
            buffer.msg_bytes_mut()[i] = i as u8;
        }
    });

    ep.send(MessageInfo::new(0x1234, 0, 0, 3));

    BootInfo::init_thread_tcb().debug_name(b"test_after");
    debug_println!("send ipc buffer done");
    r#yield();

    unsafe {
        (0x12345678 as *mut u8).write_volatile(0);
    }

    ep.send(MessageInfo::new(
        CustomMessageLabel::Exit.to_label(),
        0,
        0,
        0,
    ));
    loop {
        r#yield()
    }
}

pub fn sys_null(sys: isize) {
    unsafe {
        core::arch::asm!("svc 0",
            in("x7") sys,
        );
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    loop {}
}
