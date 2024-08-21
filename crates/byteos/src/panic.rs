// use backtrace::backtrace;
extern crate sel4_panicking;

// 程序遇到错误
// #[panic_handler]
// fn panic_handler(info: &PanicInfo) -> ! {
//     if let Some(location) = info.location() {
//         println!(
//             "\x1b[1;31m[Core {}] [{}:{}]\x1b[0m",
//             hart_id(),
//             location.file(),
//             location.line(),
//         );
//     }
//     println!(
//         "\x1b[1;31m[Core {}] panic: '{}'\x1b[0m",
//         hart_id(),
//         info.message().unwrap()
//     );
//     // backtrace();
//     println!("!TEST FINISH!");
//     // loop {}
//     Instruction::shutdown()
// }

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    sel4::debug_println!("Task Error");
    loop {}
}
