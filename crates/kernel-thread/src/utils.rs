use alloc::format;
use sel4::debug_println;

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

pub const GRANULE_SIZE: usize = sel4::FrameObjectType::GRANULE.bytes();

#[repr(C, align(4096))]
struct FreePagePlaceHolder(#[allow(dead_code)] [u8; GRANULE_SIZE]);

/// 空闲页
static mut FREE_PAGE_PLACEHOLDER: FreePagePlaceHolder = FreePagePlaceHolder([0; GRANULE_SIZE]);

pub unsafe fn init_free_page_addr() -> usize {
    core::ptr::addr_of!(FREE_PAGE_PLACEHOLDER) as _
}
