use core::ops::Range;
use core::ptr;
use sel4::{InitCSpaceSlot, LocalCPtr, SizedFrameType};

// use crate::heap::HEAP_MEM;

pub struct UserImageUtils;

static mut BOOT_INFO: usize = 0;
static mut HEAP_P_V_OFFSET: usize = 0;

impl UserImageUtils {
    pub fn init(&self, bootinfo: &sel4::BootInfo) {
        unsafe {
            BOOT_INFO = bootinfo as *const sel4::BootInfo as usize;
            // debug_println!("heap start: {:#x}, heap end: {:#x}", HEAP_MEM.as_ptr() as usize, HEAP_MEM.as_ptr() as usize + (1 << 24));
            // let paddr_start = self.get_user_image_frame_paddr(HEAP_MEM.as_ptr() as usize);
            // HEAP_P_V_OFFSET = paddr_start - HEAP_MEM.as_ptr() as usize;
        }
    }
    pub fn get_user_image_frame_slot(&self, vaddr: usize) -> InitCSpaceSlot {
        assert_eq!(vaddr % GRANULE_SIZE, 0);
        let user_image_footprint = get_user_image_footprint();
        let bootinfo = unsafe { &*(BOOT_INFO as *const sel4::BootInfo) };
        let num_user_frames = bootinfo.user_image_frames().len();
        assert_eq!(user_image_footprint.len(), num_user_frames * GRANULE_SIZE);
        let ix = (vaddr - user_image_footprint.start) / GRANULE_SIZE;
        bootinfo.user_image_frames().start + ix
    }

    pub fn get_user_image_frame_paddr(&self, vaddr: usize) -> usize {
        let offset = vaddr % 4096;
        let new_vaddr = vaddr - offset;
        let frame_cap = self.get_user_image_frame_slot(new_vaddr);
        let frame = LocalCPtr::<sel4::cap_type::SmallPage>::from_bits(frame_cap as u64);
        sel4_logging::log::info!("frame: {:#x?}", frame.frame_get_address());
        frame.frame_get_address().unwrap() + offset
    }

    #[inline]
    pub fn get_heap_paddr(vaddr: usize) -> usize {
        // unsafe {
        //     vaddr + HEAP_P_V_OFFSET
        // }
        UserImageUtils.get_user_image_frame_paddr(vaddr)
    }

    #[inline]
    pub fn get_heap_vaddr(paddr: usize) -> usize {
        unsafe { paddr - HEAP_P_V_OFFSET }
    }
}

fn get_user_image_footprint() -> Range<usize> {
    extern "C" {
        static __executable_start: u64;
        static _end: u64;
    }
    unsafe {
        let start = round_down(ptr::addr_of!(__executable_start) as usize, GRANULE_SIZE);
        let end = (ptr::addr_of!(_end) as usize).next_multiple_of(GRANULE_SIZE);
        start..end
    }
}

const fn round_down(n: usize, b: usize) -> usize {
    n - n % b
}

const GRANULE_SIZE: usize = sel4::cap_type::Granule::FRAME_SIZE.bytes();
