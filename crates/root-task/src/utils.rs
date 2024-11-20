use sel4::{init_thread, AbsoluteCPtr, HasCPtrWithDepth};

/// Send a syscall to sel4 with none arguments
#[allow(dead_code)]
pub fn sys_null(sys: isize) {
    unsafe {
        core::arch::asm!(
            "svc 0",
            in("x7") sys,
        );
    }
}

/// Get [AbsoluteCPtr] from current CSpace though path.
pub fn abs_cptr<T: HasCPtrWithDepth>(path: T) -> AbsoluteCPtr {
    init_thread::slot::CNODE.cap().relative(path)
}

pub const GRANULE_SIZE: usize = sel4::FrameObjectType::GRANULE.bytes();

#[repr(C, align(4096))]
struct FreePagePlaceHolder(#[allow(dead_code)] [u8; GRANULE_SIZE]);

/// 空闲页
static mut FREE_PAGE_PLACEHOLDER: FreePagePlaceHolder = FreePagePlaceHolder([0; GRANULE_SIZE]);

/// unmap 空闲页，返回该页起始地址
pub unsafe fn init_free_page_addr(bootinfo: &sel4::BootInfo) -> usize {
    let addr = core::ptr::addr_of!(FREE_PAGE_PLACEHOLDER) as usize;
    get_user_image_frame_slot(bootinfo, addr)
        .cap()
        .frame_unmap()
        .unwrap();
    addr
}

fn get_user_image_frame_slot(
    bootinfo: &sel4::BootInfo,
    addr: usize,
) -> sel4::init_thread::Slot<sel4::cap_type::Granule> {
    extern "C" {
        static __executable_start: usize;
    }
    let user_image_addr = core::ptr::addr_of!(__executable_start) as usize;
    bootinfo
        .user_image_frames()
        .index(addr / GRANULE_SIZE - user_image_addr / GRANULE_SIZE)
}
