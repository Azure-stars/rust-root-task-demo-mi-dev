use alloc::format;
use crate_consts::GRANULE_SIZE;
use sel4::debug_println;

use crate::FREE_PAGE_PLACEHOLDER;

pub fn print_test(title: &str) {
    debug_println!("{:=^60}", format!(" {} BEGIN", title));
}

pub fn print_test_end(title: &str) {
    debug_println!("{:=^60}", format!(" {} PASSED ", title));
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
pub fn align_bits<T: Into<usize> + From<usize>>(a: T, b: usize) -> T {
    (a.into() & !((1 << b) - 1)).into()
}

#[repr(C, align(4096))]
pub struct FreePagePlaceHolder(#[allow(dead_code)] pub [u8; GRANULE_SIZE]);

pub unsafe fn init_free_page_addr() -> usize {
    core::ptr::addr_of!(FREE_PAGE_PLACEHOLDER) as _
}
