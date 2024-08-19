#![no_std]
#![no_main]

mod buddy;

pub use buddy::HeapAllocator;

#[macro_export]
macro_rules! defind_allocator {
    ($(#[$attr:meta])* ($name:ident, $size:expr)) => {
        /// Rust Global Allocator implement.
        $(#[$attr])*
        #[global_allocator]
        static $name: $crate::HeapAllocator<$size> = $crate::HeapAllocator::new();
    };
}
