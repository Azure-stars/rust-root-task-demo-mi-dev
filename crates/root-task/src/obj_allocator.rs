use core::ops::Range;

use sel4::{
    cap_type, BootInfo, CapType, ObjectBlueprint, ObjectBlueprintAArch64, ObjectBlueprintArch,
    Untyped,
};
use spin::Mutex;

pub(crate) struct ObjectAllocator {
    empty_slots: Range<usize>,
    ut: sel4::Untyped,
}

pub trait AllocateCapBluePrint {
    fn get_blueprint() -> Option<ObjectBlueprint>;
}

pub trait AllocateCapBluePrintSized {
    fn get_blueprint(size_bits: usize) -> ObjectBlueprint;
}

macro_rules! impl_cap_blueprint {
    ($cap:ty) => {
        impl AllocateCapBluePrint for $cap {
            fn get_blueprint() -> Option<ObjectBlueprint> {
                None
            }
        }
    };
    ($cap:ty, $bp: expr) => {
        impl AllocateCapBluePrint for $cap {
            fn get_blueprint() -> Option<ObjectBlueprint> {
                Some($bp.into())
            }
        }
    };
    ($cap:ty, $bp: expr, $bits_name: ident) => {
        impl AllocateCapBluePrintSized for $cap {
            fn get_blueprint($bits_name: usize) -> ObjectBlueprint {
                $bp.into()
            }
        }
    };
}

pub static OBJ_ALLOCATOR: Mutex<ObjectAllocator> = Mutex::new(ObjectAllocator::empty());

impl_cap_blueprint!(cap_type::IRQHandler);
impl_cap_blueprint!(cap_type::Endpoint, ObjectBlueprint::Endpoint);
impl_cap_blueprint!(cap_type::TCB, ObjectBlueprint::TCB);
impl_cap_blueprint!(cap_type::VSpace, ObjectBlueprintAArch64::VSpace);
impl_cap_blueprint!(cap_type::SmallPage, ObjectBlueprintArch::SmallPage);
impl_cap_blueprint!(cap_type::PT, ObjectBlueprintArch::PT);
impl_cap_blueprint!(cap_type::Notification, ObjectBlueprint::Notification);
impl_cap_blueprint!(
    cap_type::CNode,
    ObjectBlueprint::CNode { size_bits },
    size_bits
);

impl ObjectAllocator {
    /// Create a new empty ObjectAllocator
    pub(crate) const fn empty() -> Self {
        Self {
            empty_slots: 0..0,
            ut: sel4::Untyped::from_bits(0),
        }
    }

    /// Init object allocator with [sel4::BootInfo]
    pub(crate) fn init(&mut self, empty_range: Range<usize>, untyped: Untyped) {
        self.empty_slots = empty_range;
        self.ut = untyped;
    }

    /// Allocate the slot and cnode cnode slot.
    #[inline]
    pub(crate) fn alloc_slot(&mut self) -> (usize, usize, usize) {
        let raw_slot_index = self.empty_slots.next().unwrap();
        let slot_index = raw_slot_index & 0xfff;
        let cnode_index = raw_slot_index >> 12;

        if slot_index == 0 {
            self.ut
                .untyped_retype(
                    &ObjectBlueprint::CNode { size_bits: 12 },
                    &BootInfo::init_thread_cnode().relative_self(),
                    cnode_index,
                    1,
                )
                .expect("can't allocate notification");
        }

        (slot_index, cnode_index, raw_slot_index)
    }
}

/// Allocate cap with Generic definition.
pub(crate) fn alloc_cap<C: AllocateCapBluePrint + CapType>() -> sel4::LocalCPtr<C> {
    let mut allocator = OBJ_ALLOCATOR.lock();
    // let slot_index = allocator.empty_slots.next().unwrap();
    let (slot_index, cnode_index, raw) = allocator.alloc_slot();
    if let Some(ref blue_print) = C::get_blueprint() {
        allocator
            .ut
            .untyped_retype(
                blue_print,
                &BootInfo::init_thread_cnode().relative_bits_with_depth(cnode_index as _, 52),
                slot_index,
                1,
            )
            .expect("can't allocate notification");
    }
    sel4::BootInfo::init_cspace_local_cptr::<C>(raw)
}

/// Allocate cap with Generic definition and size_bits;
pub(crate) fn alloc_cap_size<C: AllocateCapBluePrintSized + CapType>(
    size_bits: usize,
) -> sel4::LocalCPtr<C> {
    let mut allocator = OBJ_ALLOCATOR.lock();
    // let slot_index = allocator.empty_slots.next().unwrap();
    let (slot_index, cnode_index, raw) = allocator.alloc_slot();
    allocator
        .ut
        .untyped_retype(
            &C::get_blueprint(size_bits),
            &BootInfo::init_thread_cnode().relative_bits_with_depth(cnode_index as _, 52),
            slot_index,
            1,
        )
        .expect("can't allocate notification");
    sel4::BootInfo::init_cspace_local_cptr::<C>(raw)
}

/// Allocate cap with Generic definition and size_bits;
pub(crate) fn alloc_cap_size_slot<C: AllocateCapBluePrintSized + CapType>(
    size_bits: usize,
) -> sel4::LocalCPtr<C> {
    let mut allocator = OBJ_ALLOCATOR.lock();
    let slot_index = allocator.empty_slots.next().unwrap();
    allocator
        .ut
        .untyped_retype(
            &C::get_blueprint(size_bits),
            &BootInfo::init_thread_cnode().relative_self(),
            slot_index,
            1,
        )
        .expect("can't allocate notification");
    sel4::BootInfo::init_cspace_local_cptr::<C>(slot_index)
}
