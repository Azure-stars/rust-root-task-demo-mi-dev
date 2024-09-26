//! Rust Global Allocator implement.
//!
use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::NonNull,
};

use buddy_system_allocator::LockedHeap;

/// The default heap size of the global allocator
// const DEFAULT_HEAP_SIZE: usize = 0x1_0000;
/// The minimum heap threshold of the global allocator
const MEMORY_THRESHOLD: usize = 4096;

/// Heap Allocator for QuadOS.
#[repr(align(4096))]
pub struct HeapAllocator<const DEFAULT_SIZE: usize> {
    data: [u8; DEFAULT_SIZE],
    heap: LockedHeap<32>,
}

impl<const S: usize> HeapAllocator<S> {
    pub const fn new() -> Self {
        Self {
            data: [0u8; S],
            heap: LockedHeap::new(),
        }
    }
}

/// Implement GlobalAlloc for HeapAllocator.
unsafe impl<const DEFAULT_SIZE: usize> GlobalAlloc for HeapAllocator<DEFAULT_SIZE> {
    /// Allocate the memory from the allocator.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Get heap usage
        let (total, actual) = {
            let heap = self.heap.lock();
            (heap.stats_total_bytes(), heap.stats_alloc_actual())
        };

        // Supply heap allocator's memory
        if total == 0 {
            let mm_start = self.data.as_ptr() as usize;
            self.heap
                .lock()
                .add_to_heap(mm_start, mm_start + DEFAULT_SIZE);
        } else if total - actual < layout.size() + MEMORY_THRESHOLD {
            // TODO: Allocate memory if memory is not enough available
        }

        // Allocate memory
        self.heap
            .lock()
            .alloc(layout)
            .ok()
            .map_or(core::ptr::null_mut(), |allocation| allocation.as_ptr())
    }

    /// DeAllocate the memory from the allocator.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.heap
            .lock()
            .dealloc(NonNull::new_unchecked(ptr), layout)
    }
}
