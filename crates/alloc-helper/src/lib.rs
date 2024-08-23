#![doc = include_str!("../README.md")]
#![no_std]
#![no_main]

mod buddy;

pub use buddy::HeapAllocator;

/// Define a heap allocator for rust alloc crate.
#[macro_export]
macro_rules! define_allocator {
    ($(#[$attr:meta])* ($name:ident, $size:expr)) => {
        /// Rust Global Allocator implement.
        $(#[$attr])*
        #[global_allocator]
        static $name: $crate::HeapAllocator<$size> = $crate::HeapAllocator::new();
    };
}
