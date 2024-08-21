use common::RootMessageLabel;
use core::ptr::NonNull;
use sel4::{debug_println, Endpoint};
use virtio_drivers::{BufferDirection, Hal, PhysAddr};

pub struct HalImpl;

unsafe impl Hal for HalImpl {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        debug_println!("DMA Alloc Page: {}", pages);

        let ep = Endpoint::from_bits(18);
        let root_message = RootMessageLabel::try_from(
            &ep.call(RootMessageLabel::TranslateAddr(0x1_0000_3000).build()),
        );

        match root_message {
            Some(RootMessageLabel::TranslateAddr(addr)) => {
                (addr, NonNull::new(0x1_0000_3000 as *mut u8).unwrap())
            }
            _ => todo!(),
        }
    }

    unsafe fn dma_dealloc(_paddr: PhysAddr, _vaddr: NonNull<u8>, _pages: usize) -> i32 {
        // D::dealloc(paddr, pages)
        todo!()
    }

    unsafe fn mmio_phys_to_virt(_paddr: PhysAddr, _size: usize) -> NonNull<u8> {
        // NonNull::new(D::phys_to_virt(paddr) as _).unwrap()
        todo!()
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> PhysAddr {
        let ep = Endpoint::from_bits(18);
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
