use axdriver_virtio::{BufferDirection, PhysAddr, VirtIoHal};
use common::RootMessageLabel;
use core::{
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};
use crate_consts::{DEFAULT_THREAD_FAULT_EP, DMA_ADDR_START, PAGE_SIZE};
use sel4::{cap::Endpoint, debug_println};
static DMA_ADDR: AtomicUsize = AtomicUsize::new(DMA_ADDR_START + PAGE_SIZE);

pub struct VirtIoHalImpl;

unsafe impl VirtIoHal for VirtIoHalImpl {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        debug_println!("[Net Thread] DMA Alloc Page: {}", pages);
        let vaddr = DMA_ADDR.load(Ordering::Acquire);
        DMA_ADDR.store(vaddr + pages * PAGE_SIZE, Ordering::Release);
        let ep = Endpoint::from_bits(DEFAULT_THREAD_FAULT_EP);
        let root_message =
            RootMessageLabel::try_from(&ep.call(RootMessageLabel::TranslateAddr(vaddr).build()));

        match root_message {
            Some(RootMessageLabel::TranslateAddr(paddr)) => {
                (paddr, NonNull::new(vaddr as *mut u8).unwrap())
            }
            _ => todo!(),
        }
    }

    unsafe fn dma_dealloc(_paddr: PhysAddr, vaddr: NonNull<u8>, pages: usize) -> i32 {
        let vaddr = vaddr.as_ptr() as usize;
        let pre_addr = DMA_ADDR.load(Ordering::Acquire);
        assert!(vaddr + pages * 0x1000 == pre_addr);
        DMA_ADDR.store(vaddr, Ordering::Release);
        0
    }

    unsafe fn mmio_phys_to_virt(_paddr: PhysAddr, _size: usize) -> NonNull<u8> {
        // NonNull::new(D::phys_to_virt(paddr) as _).unwrap()
        todo!()
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> PhysAddr {
        let ep = Endpoint::from_bits(DEFAULT_THREAD_FAULT_EP);

        let root_message = RootMessageLabel::try_from(
            &ep.call(RootMessageLabel::TranslateAddr(buffer.as_ptr() as *const u8 as _).build()),
        );

        // TODO: Translate buffer to physical address
        match root_message {
            Some(RootMessageLabel::TranslateAddr(addr)) => addr,
            _ => todo!(),
        }
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection) {
        // Nothing to do, as the host already has access to all memory and we didn't copy the buffer
        // anywhere else.
    }
}
