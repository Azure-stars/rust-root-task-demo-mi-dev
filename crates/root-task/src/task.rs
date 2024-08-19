use sel4::cap_type;
use task_helper::{Sel4TaskHelper, TaskHelperTrait};

use crate::{obj_allocator::alloc_cap, page_seat_vaddr};

pub type Sel4Task = Sel4TaskHelper<TaskImpl>;

pub struct TaskImpl;

impl TaskHelperTrait<Sel4TaskHelper<Self>> for TaskImpl {
    const IPC_BUFFER_ADDR: usize = 0x1_0000_1000;
    const DEFAULT_STACK_TOP: usize = 0x1_0000_0000;
    fn page_seat_vaddr() -> usize {
        page_seat_vaddr()
    }

    fn allocate_pt(_task: &mut Self::Task) -> sel4::PT {
        alloc_cap::<cap_type::PT>()
    }

    fn allocate_page(_task: &mut Self::Task) -> sel4::SmallPage {
        alloc_cap::<cap_type::SmallPage>()
    }
}
