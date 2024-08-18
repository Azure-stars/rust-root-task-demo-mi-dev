use alloc::format;
use sel4::debug_println;

pub fn print_test(title: &str) {
    debug_println!("{:=^60}", format!(" {} ", title));
}

pub fn print_test_end(title: &str) {
    debug_println!("{:=^60}", format!(" {} END ", title));
}

#[macro_export]
macro_rules! test_func {
    ($title: literal, $test:block) => {{
        crate::utils::print_test($title);
        $test;
        crate::utils::print_test_end($title);
    }};
    ($title: literal, $test:expr) => {
        test_func!($title, { $test })
    };
}

/// Align a with b bits
pub const fn align_bits<T: Into<usize> + From<usize>>(a: T, b: usize) -> T {
    (a.into() & !((1 << b) - 1)).into()
}
