#import "simple-template.typ": *

#show: project.with(
  title: "Rust seL4 指北",
  authors: (
    (
      name: "yfblock",
      // organization: [],
      email: "321353225@qq.com"
    ),
  ), 
  abstract: "本文简要介绍了怎么使用 Rust seL4 创建一个简单的应用程序"
)



#pagebreak()

= 介绍

== Rust seL4 介绍

Rust-seL4@rust-sel4 是帮助 Rust 开发者在用户空间中编写应用程序的包，包括：

+ seL4 Api 的 bindings
+ root-task tools.
+ kernel-loader
+ 许多用于 seL4 用户空间的 crate
+ Microkit 工具集的 Rust bindings

下面是通用的 Rust seL4@rust-sel4 crates 介绍。

#figure(
  table(
    columns: 2,
    align: left,
    table.header(
    [CrateName],[Description]
    ),

    [sel4], [ Straightforward, pure-Rust bindings to the seL4 API.],
    [sel4-sys], [ Raw bindings to the seL4 API, generated from the libsel4 headers and interface definition files. This crate is not intended to be used directly by application code, but rather serves as a basis for the sel4 crate's implementation.],
    [sel4-config], [ Macros and constants corresponding to the seL4 kernel configuration. Can be used by all targets (i.e. in all of: application code, build scripts, and build-time tools).],
    [sel4-platform-info], [ Constants corresponding to the contents of platform_info.h. Can be used by all targets.],
    [sel4-sync], [ Synchronization constructs using seL4 IPC. Currently only supports notification-based mutexes.],
    [sel4-logging], [ Log implementation for the log crate.],
    [sel4-externally-shared], [ Abstractions for interacting with data structures in shared memory.],
    [sel4-shared-ring-buffer], [ Implementation of shared data structures used in the seL4 Device Driver Framework.],
    [sel4-async-\*], [ Crates for leveraging async Rust in seL4 userspace.],
  ),
  caption: "通用 crates 表"
)

下面是 Runtime Crates:

#figure(
  table(
    columns: 2,
    align: left,
    table.header([CrateName], [Description]),
    [sel4-root-task], [A runtime for root tasks that supports thread-local storage and unwinding, and provides a global allocator.],
    [sel4-microkit], [A runtime for seL4 Microkit protection domains, including an implementation of libmicrokit and abstractions for IPC.]
  ),
  caption: "用于 Runtime 的 crates 表"
)

Rust seL4@rust-sel4 工具。

#figure(
  table(
    columns: 2,
    align: left,
    table.header([CrateName], [Description]),

    [sel4-capdl-initializer], [A CapDL-based system initializer.],
    [sel4-kernel-loader], [A loader for the seL4 kernel, similar in purpose to elfloader.]
  ),
  caption: "工具集"
)

== Task 介绍

seL4@sel4 中没有进程和线程的明确区分，也没有像宏内核中那样明确的概念，甚至#link("https://docs.sel4.systems/Tutorials/threads.html") 也是使用 `Thread` 来描述任务。

在 seL4@sel4 中创建一个进程就是让一个 TCB 拥有全新的资源，创建一个线程就是让一个 TCB 和另一个 TCB 共享资源。@process-thread-img 中展示了进程和线程的关系，在 seL4@sel4 中创建一个线程就是创建一个新的 TCB 和全新的 _CNode_, _CSpace_, _VSpace_ 等资源。在 seL4@sel4 中没有非常明确的进程的定义，只有逻辑上的进程，同一个进程中的多个线程之间共享资源。

#figure(
  image("imgs/thread.excalidraw.png", width: 50%),
  caption: "进程和线程关系图"
) <process-thread-img>

当然每个线程还有其自己独立的上下文结构以及调度优先级。在 seL4@sel4 中已经提供了非常多的 API 来进行 Task 相关的操作。详细的信息将在 @create-thread-ch 中进行描述。


== CSpace 介绍

CSpace 是 seL4@sel4 中的重要概念，CSpace 和 VSpace 不仅名称相似，存储和寻址方式也是非常相似。@cspace-struct-img 中提供了一个 32 位寻址时 CSpace 的一个简单案例。

#figure(
  image("imgs/cspace.png", width: 90%),
  caption: "CSpace 结构图"
) <cspace-struct-img>

@cspace-struct-img 中给出了一个 CSpace， 这个 CSpace 由三级 CNode 组成，CSpace 中由 A, B, C, D, E, F, G 等 7 个有效的 Capability。这个结构与 VSpace 中的地址空间十分相似。

下面给出一些案例，怎么查找特定的 Capability。下面所有的查找深度都指定为 32。

- *寻找 Cap A* #h(0.5em) 查找地址为 0x06000000，为什么是这个值呢？因为从 L1 CNode Cap 进来，首先是一个 Guard，这个 Guard 是 4bit 也就是一个 16 进制位，而图中的 Guard 是 0，所以就是 0, 然后 Cap A 在 60 处，所以我们拼出的前三个数字就是 060，此时已经找到了 Cap A，后面的地址直接补充为 0 即可，也就是 0x06000000。
- *寻找 Cap B* #h(0.5em) Cap B 和 Cap A 有些差距，因为 Cap B 是在第二级的 CNode 中，虽然看起来有点麻烦，但是聪明的你一定想到了解决的方法。Cap B 的查找地址为 0x00f06000。下面是原因，在 L1 CNode Cap 中 0x0f 中存在一个子 CNode, 图中显示这是 L2 CNode Cap, 但是这只是一个普通的 CNode, 你可以理解为这是一个树状结构。所以要查找 Cap B 首先是在 L1 CNode Cap 中找到 L2 的入口，Guard 是 4bit 0, L2 CNode Cap 在 0x0f 中，所以最高三个十六进制数为 0x00f，然后在 L2 CNode Cap 中查找，L2 CNode Cap 的 Guard 为 4bit 0, Cap B 在 0x60 中。所以查找地址就是 0x00f06000。
- *寻找 Cap C..* #h(0.5em) 这里直接给出答案，过程就让聪明的你自己推敲了。 C: 0x00f00060. D: 0x00f00061 ...

== Untyped Memory

#pagebreak()

= 线程 <create-thread-ch>

== RootTask

RootTask 是 seL4@sel4 中启动的第一个用户态程序，RootTask 掌握了操作系统所有的资源，RootTask 可以进行资源的分配。*RootTask 在启动的时候只有一级 Slot*，seL4@sel4 默认 CNode 大小只有 12 Bits，也就是只能存放 $2^12$ 个 Slot，而页表是 4k 映射的, 所以 Root-task 的大小最好不要超过 16M，否则需要去改内核的一个 CONFIG_ROOT_CNODE_SIZE_BITS 去扩展 Root Task 的 bits。

=== 扩展 Root Task CSpace

默认 root-task 的 CSpace 是一级，只有 12 bits，在我们构建一些大型应用的时候是不够用的。所以需要对 root-task 的 CSpace 进行扩展，扩展为二级的逻辑如下:

+ 创建一个新的 CNode 作为新的 ROOT CNode
+ 将原来的 CNode 映射到新的 CNode 中的第一个 Slot，这样能保证扩展后能跟原 CSpace 保持一致
+ 更新 rust-seL4@rust-sel4 中规定的 Init CNode Slot
+ 更新 root-task 的 TCB 中的 CSpace 设置

下面给出代码的示例(`CNODE_RADIX_BITS = 12`):

```rust
// 申请一个新的 CNode
let cnode = alloc_cap_size_slot::<cap_type::CNode>(CNODE_RADIX_BITS);

// 将原来的 CNode 复制新 CNode 的第一个 Slot
cnode
    .relative_bits_with_depth(0, CNODE_RADIX_BITS)
    .mint(
        &BootInfo::init_thread_cnode().relative(BootInfo::init_thread_cnode()),
        CapRights::all(),
        CNodeCapData::skip(0).into_word(),
    )
    .unwrap();

// 将原来的 Slot 移动到临时的 Slot 中
BootInfo::init_thread_cnode()
    .relative(BootInfo::null())
    .mutate(
        &BootInfo::init_thread_cnode().relative(BootInfo::init_thread_cnode()),
        CNodeCapData::skip_high_bits(CNODE_RADIX_BITS).into_word(),
    )
    .unwrap();

// 更新 rust-seL4 中规定的 Init CNode Slot
CNode::from_bits(0)
    .relative(BootInfo::init_thread_cnode())
    .mint(
        &CNode::from_bits(0).relative(cnode),
        CapRights::all(),
        CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word(),
    )
    .unwrap();

// 删除临时 Slot
BootInfo::init_thread_cnode()
    .relative(BootInfo::null())
    .delete()
    .unwrap();

// 更新 task 的 CSpace
BootInfo::init_thread_tcb().invoke(|cptr, buffer| {
    buffer.inner_mut().seL4_TCB_SetSpace(
        cptr.bits(),
        BootInfo::null().cptr().bits(),
        cnode.bits(),
        CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word(),
        BootInfo::init_thread_vspace().bits(),
        0,
    )
});
```

这时 root-task 的 CSpace 已经扩展为两级。使用方式和之前完全一致。如果前 4096 个 Slot 已经使用完毕。下面给出扩展第二个 4096 的方法：

```rust

let cnode = alloc_cap_size_slot::<cap_type::CNode>(CNODE_RADIX_BITS);

BootInfo::init_thread_cnode()
    .relative_bits_with_depth(1, CNODE_RADIX_BITS)
    .mint(
        &BootInfo::init_thread_cnode().relative(cnode),
        CapRights::all(),
        CNodeCapData::skip(0).into_word(),
    )
    .unwrap();
```

== 创建线程

需要创建一个新的线程需要提供以下资源：

- *Priority* 线程的优先级
- *Max control Priority* 线程可控制的最大优先级（即这个线程自己产生的线程所能赋予的最大优先级）
- *Registers* 寄存器状态，包括浮点寄存器。（这些默认都会有一个初始值，只需要修改需要修改的就行了）
- *VSpace Capability* VSpace 能力
- *VSpace Capability* CSapce 能力
- *fault_endpoint* 一个 EP(endpoint) (当前任务发生错误时，内核会通过这个 EP 发送错误信息)
- *IPC_Buffer* IPC Buffer 地址和 Capability

=== 使用 rust-seL4 库创建任务

根据上面描述的信息，创建一个新的线程需要提供 CNode、VSpace、FaultEP 等资源。

```rust
// 申请任务需要的 Capability
let mut task = Sel4Task::new();
let ep = alloc_cap::<cap_type::Endpoint>();
let tcb = alloc_cap::<cap_type::TCB>();
let vspace = alloc_cap::<cap_type::VSpace>();
let root_cnode =alloc_cap_size::<cap_type::CNode>(12);

// 配置任务， TIPS: IPC Buffer 为空，不可用
tcb.tcb_configure(
    ep.cptr(),
    root_cnode,
    CNodeCapData::new(0, sel4::WORD_SIZE - 12),
    vspace,
    0,
    Granule::from_bits(0),
)?;
// 设置子任务的优先级和控制优先级
// 优先级为 255, 控制优先级为 0
task.tcb
    .tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 0, 255)?;

// 设置上下文信息
let mut user_context = sel4::UserContext::default();
*user_context.pc_mut() = todo!("Program Entry");
tcb
  .tcb_write_all_registers(false, &mut user_context)
  .unwrap();

// 运行任务
task.tcb.tcb_resume().unwrap();

```

=== 使用再封装的库创建任务

rust-seL4@rust-sel4 提供的接口虽然很丰富，但是直接使用就会很麻烦。因此在 rust-seL4@rust-sel4 上对接口进行再封装迫在眉睫，封装后的 crate 在 https://github.com/rel4team/rust-root-task-demo-mi-dev/blob/docs/crates/task-helper/README.md

```rust
// 申请资源
let cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
let inner_cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
let fault_ep = (alloc_cap::<cap_type::Endpoint>(), 0);
let tcb = alloc_cap::<cap_type::TCB>();
let vspace = alloc_cap::<cap_type::VSpace>();

// 创建任务
let mut task = Sel4Task::new(tcb, cnode, fault_ep.0, vspace, fault_ep.1);

// 配置任务
task.configure(CNODE_RADIX_BITS)?;

// 映射 10 个页表在作为栈
task.map_stack(10);

// 设置任务优先级和控制优先级
task.tcb
    .tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 0, 255)?;

// 映射 elf 文件到任务的地址空间
task.map_elf(elf_file);

// 运行任务
task.tcb.tcb_resume().unwrap();
```

== 运行任务

在创建任务后，任务默认处于等待运行的状态，如果需要运行程序，需要调用特定的接口操作 TCB。

```rust
tcb.tcb_resume().unwrap();
```

即便是再封装的结构体，也可以直接使用封装的 tcb 进行操作。封装的结构体如下

```rust
pub struct Sel4TaskHelper<H: TaskHelperTrait<Self>> {
    pub tcb: sel4::TCB,
    pub cnode: sel4::CNode,
    pub vspace: sel4::VSpace,
    pub mapped_pt: Arc<NotiMutex<Vec<sel4::PT>>>,
    pub mapped_page: BTreeMap<usize, sel4::Granule>,
    pub stack_bottom: usize,
    pub phantom: PhantomData<H>,
}
```

相关的介绍如 @tcb-helper-field-table 所示，所有的 field 都是 pub，可以直接使用。

#figure(
  table(columns: 2,
    align: left,
    table.header(
      [FieldName], [Description],
    ),
    [tcb], [tcb Capacity 参考 sel4::TCB],
    [cnode], [cnode Capacity 参考 sel4::CNode],
    [vspace], [vspace Capacity 参考 sel4::VSpace],
    [mapped_pt], [已经映射的 PT 结构， 在映射 page_table 的时候需要先映射 pt，映射的 pt 会存放在这里，以便任务结束时释放],
    [mapped_page], [已经映射的 SmallPage 别名为 Granule，是 map 结构，映射的地址为 key(4k 对齐)，page cap 为值],
    [stack_bottom], [当前栈底，这里适用于初始化栈赋值和更新],
    [phantom], [对 trait 实现进行存储，参考 rust PhantomData],

  )
)<tcb-helper-field-table>

当程序运行时发生错误，内核会将错误通过 fault_ep 发送错误信息，其他拥有这个 fault_ep Cap 的任务可以监听错误信息并进行处理。错误发生时 tcb 处于 block 状态，可以再次调用 `tcb_resume` 恢复任务的运行。后面的章节 @missing-page-ch 中将利用缺页错误简单演示这种机制。

== 销毁任务

再 seL4@sel4 中任务的运行非常的简单，同样任务的停止也非常的简单，直接调用 `tcb_suspend` 就可以将任务停止。至于如何再 seL4@sel4 中看到 task 的运行状态，在 seL4@sel4 中有一个用于 debug 的 syscall 存在，但是 rust-seL4@rust-sel4 中并没有提供这个函数的封装，我们可以直接自己构造相应的调用。

下面给出一个 `aarch64` 架构下 `syscall` 的封装。

```rust
/// 发送一个没有参数的 Syscall
pub fn sys_null(sys: isize) {
    unsafe {
        core::arch::asm!(
            "svc 0",
            in("x7") sys,
        );
    }
}
```

只有我们就可以利用这个函数调用 SysCall 了。DebugDumpScheduler 就是我们需要使用的 SysCall，SysCall id 号为 -10，这个 SysCall 能够显示 Scheduler 中的任务信息，@scheduler-img 是一个案例。使用 `sys_null(-10)`, 有三个还没有运行的子任务。

#figure(
  image("imgs/scheduler.png", width: 80%),
  caption: "DebugDumpScheduler 显示信息"
) <scheduler-img>

我们就能看到当前 scheduler 的状态，如果一个任务停止了，State 就会变成 Pending，运行状态就是 Running，还没有运行过的程序状态为 inactive，至于 idle，是 scheduler 中没有程序运行时运行的任务。

停止任务只是更改一个任务的 State，那么如何销毁一个任务呢？由于我们是在 seL4@sel4 上运行程序，我们底层已经有了一个操作系统存在，很多事情就不需要我们去做，*我们只需要把当前任务的 TCB 的 Capability 完全删除就可以了（包括派生出来的 Capability )*，当 TCB 完全被销毁时，内核会将 task 从 scheduler 中删除并回收相应的资源。

== 页表映射

在 seL4@sel4 的用户态上映射页表的形式有些独特，seL4@sel4 中映射一个页表需要先映射 table，然后再映射页。下面给出一个映射页的案例（从封装的 task-helper crate 中摘取）。

```rust
for _ in 0..4 {
  let res: core::result::Result<(), sel4::Error> = page.frame_map(
      self.vspace,
      vaddr as _,
      CapRights::all(),
      VMAttributes::DEFAULT,
  );
  match res {
      Ok(_) => {
          self.mapped_page.insert(vaddr, page);
          return;
      }
      // Error::FailedLookup indicates that here was not a page table.
      Err(Error::FailedLookup) => {
          let pt_cap = H::allocate_pt(self);
          pt_cap
              .pt_map(self.vspace, vaddr, VMAttributes::DEFAULT)
              .unwrap();
          self.mapped_pt.lock().push(pt_cap);
      }
      _ => res.unwrap(),
  }
}
```

这里采用一个循环来映射页，在 aarch64 里一般使用四级页表，所以需要先映射多个 table，之后才能找到特定的虚拟页。

如果需要给特定的物理页写入内容，不能直接写入，而是需要先将需要写入的物理页映射到一个特定的虚拟页上，然后写入内存，最后取消映射。*需要注意，一个 Frame Capability 只能映射在一个虚拟页上，如果想要多个任务共享一个物理页，就需要将 Capability 进行复制，复制出来的 Capability 可以再次被映射，比如去实现 COW(Copy On Write)。*下面也给出映射 elf 文件的部分代码，同样是从封装的 task-helper 中摘取。

```rust
// 获取或者创建一个新的 Frame Capability
let page_cap = match self.mapped_page.remove(&align_page!(vaddr)) {
    Some(page_cap) => {
        page_cap.frame_unmap().unwrap();
        page_cap
    }
    None => H::allocate_page(self),
};
// 如果已经读完文件
if offset < end {
    // 将页表映射到当前 task 以写入数据
    page_cap
        .frame_map(
            BootInfo::init_thread_vspace(),
            H::page_seat_vaddr(),
            CapRights::all(),
            VMAttributes::DEFAULT,
        )
        .unwrap();

    let rsize = cmp::min(PAGE_SIZE - vaddr % PAGE_SIZE, end - offset);
    // 把数据写入到正确的物理地址
    unsafe {
        core::ptr::copy_nonoverlapping(
            file.input.as_ptr().add(offset),
            (H::page_seat_vaddr() + offset % PAGE_SIZE) as *mut u8,
            rsize,
        )
    }
    // 取消映射以便后续映射到其他地方
    page_cap.frame_unmap().unwrap();

    offset += rsize;
}

// 映射页表到任务中的特定虚拟位置，虚拟地址需要页对齐
self.map_page(align_page!(vaddr), page_cap);

// 计算偏移
vaddr += PAGE_SIZE - vaddr % PAGE_SIZE;
```

上面的代码节选自 task-helper crate 中的 map_elf 函数，上面的逻辑非常简单，就是将一个页映射到特定的虚拟地址上，如果需要写入数据，就先映射到当前的 VSpace 中，然后写入数据。*写入完毕后取消映射*，最后映射到任务的 VSpace 中。

在将物理页映射到当前任务时会将物理页映射到 H::page_seat_vaddr() 处，这是一个页对齐的地址，是一个没有任何物理页映射到的虚拟页，*如果将一个物理页映射到一个已经映射了物理页的虚拟页上，那么并不会覆盖 Capability，而是发生错误。所以最好的做法就是将这个地址放在不可能使用的地方，或者放在程序中，在开始的时候取消映射。*

这里已经将 map_page, map_elf, map_stack 等逻辑进行封装，可以直接使用。

#pagebreak()

= IPC 建立和删除

进程间通信(IPC)是用于在进程之间同步传输少量数据和能力的微内核机制。在 seL4@sel4 中，IPC 由称为 EndPoint 的小型内核对象组成，这些对象充当通用通信端口。对 EndPoint 对象的调用用于发送和接收 IPC 消息。

== IPCBuffer

rust-seL4@rust-sel4 中 IPCBuffer 的内部结构如下所示，比较需要关心的内容是 `msg`，`tag`和`caps_or_badges`。

```rust
#[repr(C)]
pub struct seL4_IPCBuffer_ {
    pub tag: seL4_MessageInfo_t,
    pub msg: [seL4_Word; 120usize],
    pub userData: seL4_Word,
    pub caps_or_badges: [seL4_Word; 3usize],
    pub receiveCNode: seL4_CPtr,
    pub receiveIndex: seL4_CPtr,
    pub receiveDepth: seL4_Word,
}
```

- *tag* 存储了消息的信息，这个需要在使用 EndPoint 传输消息的时候已经有了封装，不用特别操作。
- *msg* 存储了 IPC 需要传递的信息，这个 msg 中单位时 Word 也就是一个字，在 rust 中我们也可以认为是一个 usize。虽然不是 u8，但是我们也可以作为 u8 使用。
- *caps_or_badges* 存储需要发送的 Capability 或者 Badge, 不过还是传递 Capability 比较多一些。
- *userData* 发送的时候携带的额外消息。可以和 *msg* 搭配使用
- *receiveCNode* 当发送方的消息携带 Capability 是，携带的 Capability 将会被写入到特定 CNode 的 Index 下，写入的深度为 Depth。
- *receiveIndex* 同上协作
- *receiveDepth* 同上协作，一般不用关心，默认使用(64) 即可。

上面描述的信息是 seL4@sel4 的 bindings，在 rust-seL4@rust-sel4 中已经提供了更深度的封装。IPCBuffer 被包含在 `IPCBuffer` 内部，在使用 IPCBuffer 时也不推荐直接写入地址。而是使用 rust-seL4@rust-sel4 提供的闭包函数 `with_ipc_buffer`, `with_ipc_buffer_mut` 进行访问。

IPCBuffer 主要函数如 @ipc_buffer-interface-table 所示：

#figure(
  table(columns: 3, table.header(
    [function], [ReturnType], [Description]
  ), 
  [msg_regs(&self)], [&[Words]], [获取 msg 作为寄存器 slice], 
  [msg_regs_mut(&mut self)], [&mut [Word]], [获取 msg 作为寄存器 mutable slice], 
  [msg_bytes(&self)], [&[u8]], [获取 msg 并作为普通 buffer slice],
  [msg_bytes_mut(&mut self)], [&mut [u8]], [获取 msg 并作为普通的 mutable buffer slice],
  [user_data(&self)], [Word], [获取 IPCBuffer 中的 userData],
  [set_user_data(&mut self, data: Word)], [], [ 设置 IPCBuffer 中的 userData],
  [caps_or_badges(&self)], [&[Word]], [获取 caps_or_badges slice],
  [caps_or_badges_mut(&mut self)], [&mut [Word]], [获取 caps_or_badges mutable slice],
  [recv_slot(&self)], [AbsoluteCPtr], [获取接收 Capability 的 Slot 的绝对位置],
  [set_recv_slot(&mut self, slot: &AbsoluteCPtr)], [], [设置接收 Capability 的 Slot 位置]
  ),
  caption: "rust-seL4 中 IPCBuffer 封装的接口"
)<ipc_buffer-interface-table>

rust-seL4@rust-sel4 中已经提供了非常多的封装，已经让我们能够比较方便的在 seL4@sel4 上构建程序，但是不得不说，目前 rust-seL4@rust-sel4 中提供的一些封装无论是一些函数的命名还是操作方式，都让人感觉到十分的迷惑和繁杂。

*在新创建的任务中我们需要使用 IPCBuffer 需要先调用 set_ipc_buffer 设置 ipc_buffer 的地址，然后才能够正常的使用。*

== EndPoint

首先我们需要分配一个能够正常使用的 EndPoint。EndPoint 是一个中转站，当任务通过 EndPoint 发送数据时，发送消息的任务会阻塞（也有不阻塞的系统调用），直到 EndPoint 中的数据被另一个任务接收时才继续运行。每个消息都只有一个生产者和一个消费者。

*发送 IPC 的时候需要保证已经正确设置了 IPCBuffer，不仅仅是在创建任务的时候的 tcb_configure，还有进入任务后调用 set_ipc_buffer 设置 ipc_buffer 地址，这同时也要求任务已经正确设置了 TLS 寄存器。因此可以将 TLS 和 IPC_Buffer 看作每个任务都需要设置的必要步骤。*除了一些独特的非常简单的任务不需要设置。

EndPoint 的使用大概有以下两种情况（不考虑非阻塞）。

=== S发送 R接收

- *Send* rust-seL4 中使用 `<EndPoint>.send()` 发送数据
- *Recv* 同样的，使用 `<EndPoint>.recv()` 接收数据

发送数据的时候需要传递相应的 Message，再 rust-seL4@rust-sel4 中的类型为 `MessageInfo`。发送任务需要发送相应的 MessageInfo。前几个 MessageLabel 已经被 seL4 作为 Fault 使用，发送时可以尽量将 MessageLabel 放在更高的编号。（如果不把 FaultEP 进行复用就不会有这样的困惑，复用的好处就是只用监听一个 EndPoint，从编程上来说更加方便，可以将多个信息集中在一个 EndPoint 中处理）。

`MessageInfo` 包含以下以下字段

- *MessageLabel* 消息的标签 Label，用于标识消息的类型
- *capsUnwrapped* 
- *extraCaps* 消息中的 Capability 最大为 3，可以发送多个，但是接收只能同时接收一个
- *length* 消息中寄存器的数量，消息中寄存器和数据是同用一个 Buffer，如果是发送数据将发送数据的长度 / 8 并向上取整。


下面给出一个简单的不包括任务创建部分的案例：

```rust

// 任务1 (发送 IPC 的任务)
fn thread1(send_ep: EndPoint) {
  with_ipc_buffer_mut(|buffer| {
      buffer.msg_regs_mut()[0] = 0x123;
  });
  send_ep.send(MessageInfo::new(7, 0, 0, 1));
}

// 任务2 (接收 IPC 的任务)
fn thread2(recv_ep: EndPoint) {
  let (recv_message, _badge) = recv_ep.recv();
  assert(recv_message.label() == 7);
  assert(recv_message.length() == 1);
  with_ipc_buffer(|buffer| {
    assert(buffer.msg_regs()[0] == 0x123);
  }); 
}
```

上述案例中，我们在发送的任务 `thread1` 中发送了一个消息，消息中包含了一个寄存器的信息，消息长度为一个寄存器，消息的标签为 7。消息发送后 thread1 阻塞，等待消息被接收。当任务2 调用 recv 时会唤醒 thread1。让其状态从 block 转变为 pending。 接收到的消息会直接把 MessageInfo 返回，额外返回的还有 Badge, badge 是给 Endpoint 添加的一个特殊的标记。即便是从同一个 Endpoint 派生出的 EndPoint 也可以添加不同的标记，在 seL4@sel4 上构建传统操作系统内核的时候可以将这个 badge 设置为任务的 id，或者是任务的唯一标识，方便查找到特定的任务。

=== S发送并等待 R接收并回复

在上面的案例中我们提供了一个发送和一个接收的案例，但是上面的情况不能满足我们去让两个任务进行通信的情况，两个任务进行通信一般会有发送消息和回复消息的过程，这里给出一个案例。在发送任务中发送消息并等待，接收任务中接收消息并回复消息。

*rust-seL4@rust-sel4 中回复消息的接口并不是在 EndPoint 这个结构上，而是由一个单独的函数 reply 去使用*，而且尽量在回复消息之前不要调用其他的 IPC 接口，否则可能导致回复消息异常（如果真的需要这么做，可以去找一下 seL4@sel4 中的 saveCaller 机制）。

发送消息并等待的接口不再是上面使用的 `Send`，而是由一个新的接口 `call`， `call` 使用后会发送消息并等待消息的回复，但是 `call` 接收消息和 `recv` 不太一样， `recv` 接收消息的时候会同时接收到一个 `MessageInfo` 和一个 `badge`，但是使用 `call` 发送消息后并接受，返回的内容只有一个 `MessageInfo`，其他的回复消息的过程和发送消息的过程几乎一致。

```rust
// 任务1 (发送 IPC 的任务)
fn thread1(send_ep: EndPoint) {
  with_ipc_buffer_mut(|buffer| {
      buffer.msg_regs_mut()[0] = 0x123;
  });
  let message = send_ep.call(MessageInfo::new(7, 0, 0, 1));
  assert(message.label() == 0);
  assert(message.length() == 0);
}

// 任务2 (接收 IPC 的任务)
fn thread2(recv_ep: EndPoint) {
  let (recv_message, _badge) = recv_ep.recv();
  assert(recv_message.label() == 7);
  assert(recv_message.length() == 1);
  with_ipc_buffer(|buffer| {
    assert(buffer.msg_regs()[0] == 0x123);
  });
  // 回复消息
  with_ipc_buffer_mut(|ipc_buffer| {
      reply(ipc_buffer, MessageInfo::new(0, 0, 0, 0))
  });
}
```

== 传递更多的数据

普通的 IPCBuffer 传输能力有限，如果需要传输更多的数据。可以由下面两个选择
- 将需要传递的信息分块传输，使用多次 IPC 传递需要整个消息。
- 手动去构建共享内存来作为传输媒介，能够传输更多的数据。但是需要考虑好怎么解决抢占的问题。（如果只是两个任务之间传输就不需要考虑这个问题。）

#pagebreak()

= 缺页处理 <missing-page-ch>

这里我们构建一个缺页处理的案例来对前面描述的内容进行简单的总结. 这里直接给出连接。https://github.com/rel4team/rust-root-task-demo-mi-dev/blob/docs/crates/root-task/src/tests.rs

== 测试程序

我们需要构建一个能够触发缺页异常的任务，最简单的构造方案就是创建一个新的任务，给定一个栈指针，但是不为栈分配任何的物理页。在此处当我们运行任务的时候，就会处罚缺页错误。当我们处理完毕后还需要通知为我们处理缺页异常的任务已经运行完毕。

```rs
pub fn test_stack() {
    let mut stack = [0u8; 0x1001];
    stack[0x1000] = 1;
    unsafe {
        set_ipc_buffer(IPCBuffer::from_ptr(TaskImpl::IPC_BUFFER_ADDR as _));
    }
    debug_println!("Test Stack Successfully!");
    Notification::from_bits(DEFAULT_CUSTOM_SLOT as _).signal();
    loop {}
}
```

上面给出的任务中，有些操作是我们必要的，比如初始化 IPCBuffer,我们已经在 IPC 的章节给出过说明，如果我们需要发送 IPC 我们就需要正常初始化 IPCBuffer，这里的 Notification 也是一样，也需要初始化 IPCBuffer，因此我们构建任务的时候就需要初始化 IPCBuffer 和 tls，因为调用 set_ipc_buffer 需要正确可用的 tls。

案例中直接在栈上申请了一个叫做 stack 的数组，大小为 0x1001，且在 0x1000 出写入了一个值，这里必定会触发缺页，然后程序会处于 block 状态，在为这个任务处理缺页异常后，可以调用 resume 让这个任务继续运行。

当程序正常运行后会输出一串字符，并通过 Notification 发送 Signal。

== 管理程序

我们使用已经封装好的 task-helper crate 去创建新的任务，从工程量上来说会小很多，也会方便很多。


```rs
// 申请资源
static TLS_BUFFER: [u8; 0x100] = [0u8; 0x100];
let fault_ep = alloc_cap::<cap_type::Endpoint>();
let cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
let inner_cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
let noti = alloc_cap::<cap_type::Notification>();
let tcb = alloc_cap::<cap_type::TCB>();

// 省略构建二级 CNode 的部分
// 创建任务
let mut task = Sel4Task::new(tcb, cnode, fault_ep, BootInfo::init_thread_vspace(), 0);

// 将 Notification 复制到任务的 CNODE 中
task.abs_cptr(DEFAULT_CUSTOM_SLOT as u64)
    .copy(&abs_cptr(noti), CapRights::all())
    .unwrap();

// 配置任务
task.configure(2 * CNODE_RADIX_BITS).unwrap();

// 不映射任何栈以触发 pagefault
task.map_stack(0);

// 初始化任务的 IPC_BUFFER
task.init_ipc_buffer();

// 设置任务优先级和控制优先级 可忽略
task.tcb
    .tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 255, 255)
    .unwrap();

// 设置上下文信息（寄存器）
let mut context = UserContext::default();
*context.pc_mut() = test_stack as _;
*context.sp_mut() = crate::task::TaskImpl::DEFAULT_STACK_TOP as _;
context.inner_mut().tpidr_el0 = TLS_BUFFER.as_ptr() as _;
task.tcb
    .tcb_write_all_registers(true, &mut context)
    .unwrap();
```

此处提供了构建一个简单任务的代码，这里省略了构建二级 CSpace 的代码，如果需要找到这部分代码，在本章节的第一段中可以找到相关的链接@missing-page-ch。这里提前创建了一个 Notification 复制到任务的 CSpace，这样新创建的任务和当前任务之间就拥有了可通信的 Notification。这里给子任务提供了 0 个页作为栈，因此进入任务后必然触发缺页异常。最后为新创建的任务设置上下文（寄存器）信息，在写入寄存器的时候调用 `tcb_write_all_registers` 传递的第一个参数为 true，也就是在写入寄存器后将任务的状态更改为可运行。

新任务拥有独立的 CSpace，但是为了简化创建的流程，并未给新任务创建独立的 VSpace，因此新任务和当前任务是共享 VSpace 的。

```rust
// 接受任务的错误信息
let (message, _badge) = fault_ep.recv(());
let fault = with_ipc_buffer(|buffer| sel4::Fault::new(buffer, &message));
// 映射页表以修正缺页错误
match &fault {
    sel4::Fault::VMFault(fault) => {
        assert!(fault.addr() > 0xffff0000);
        task.map_stack(10);
        task.tcb.tcb_resume().unwrap();
    }
    _ => unreachable!(),
}
// 等待任务发送的 Notification
noti.wait();
// 释放资源
task.tcb.tcb_suspend().unwrap();
drop(task);
abs_cptr(cnode).revoke().unwrap();
abs_cptr(cnode).delete().unwrap();
abs_cptr(inner_cnode).revoke().unwrap();
abs_cptr(inner_cnode).delete().unwrap();
abs_cptr(tcb).revoke().unwrap();
abs_cptr(tcb).delete().unwrap();
debug_println!("Missing Page Handled Successfully ");
```

在任务成功运行后，就可以在当前任务接听绑定在测试程序之上的 Fault EndPoint，当测试程序发生错误时，seL4 会通过 IPC 将错误发送到与 TCB 绑定的 EndPoint 上，然后系统状态更改为 Block，其他拥有这个 Endpoint Capability 的任务可以接收到发送的错误消息，然后进行处理。

这里由于是特定构造的程序，理论上在运行时只会遇到页表缺失的错误。接收到错误后，可以尝试作为 Fault 进行读取，然后进行处理，此处接收到错误后给新任务影射了十个物理页作为栈，然后恢复任务的运行。下面等待新任务运行结束后发送的 signal。在最后给出了回收 Capability 并释放任务的操作。

#pagebreak()

= 中断注册、注销

在 seL4@sel4 中注册中断需要通过 irq_control Capability，irq_control 可以创建特定 irq 的 irq_handler，irq_handler 就是针对特定 irq 的 Capability，irq 可以绑定在 notification 上，需要等待中断的时候就调用 notification.wait() 等待。但是 irq_control 默认在 root-task 的 CSpace 中且不可派生，只能转移（也可以搞一些 magic tricks，将所有不可派生的且公共需要的 Capability 放在同一个 CNode 下，然后将CNode进行共享，没有验证过，谨慎尝试，不安全），为了保证各个任务都能够注册自己需要的中断，所以需要让各个任务通过 IPC 与 root-task 进行通信并注册 Capability，注册的方式有两种。

- 将当前空的 Slot Index 传递给 root-task，让 root-task 利用 irq_control Capability 将特定 irq 的 irq_handler 写入到这个 Slot 中。这种情况要求 root-task 掌握了需要注册任务的 CNode，这是常见的情况，也是比较简单的解决方案。
- 另一种解决方案就是将通过 IPC 转移 Capability，需要注册中断处理程序的任务提前设定好接收 Capability 的 Slot，然后发送 IPC 将需要注册的 irq 传递给 root-task，root-task 创建好特定 irq 的 irq_handler 之后将 irq_handler 通过 Capability 传递给发送 IPC 的任务。

下面给出一个简单的案例（采用第一种方式），来注册一个键盘中断，相应的例程可以打开 https://github.com/rel4team/rust-root-task-demo-mi-dev/blob/docs/crates/kernel-thread/src/irq_test.rs 进行查看，这里注册一个键盘中断。

```rust
// 串口中断号
const SERIAL_DEVICE_IRQ: usize = 33;

pub fn test_irq() {
  // 分配一个 slot 来存放 IRQHandler， 虽然指定了 cap_type 但是这个 Slot 依旧为空
  let irq_handler = alloc_cap::<cap_type::IRQHandler>();
  // 分配一个 notification, 这个真实存在
  let notification = alloc_cap::<cap_type::Notification>();
  // 从已经有的 Slot 中得到和 root-task 通信的 Endpoint.
  let ep = LocalCPtr::<cap_type::Endpoint>::from_bits(18);

  // 发送 IPC 注册中断
  ep.call(RootMessageLabel::RegisterIRQ(irq_handler.bits(), SERIAL_DEVICE_IRQ as _).build());

  // 将中断绑定在 notification 上
  irq_handler
      .irq_handler_set_notification(notification)
      .unwrap();

  // 响应中断，防止注册前的未处理中断影响
  irq_handler.irq_handler_ack().unwrap();

  // 等待中断
  debug_println!("[Kernel Thread] Waiting for irq notification");
  notification.wait();
  debug_println!("[Kernel Thread] Received irq notification");
}
```

上述过程我们申请了一个串口中断，然后通过 IPC 注册了串口中断并等待中断的响应，这个时候只需要按下任意的按键，程序即可继续运行。上述为测试的线程，下面为协助注册的 root-task 中的逻辑。

```rs
loop {
  let (message, badge) = fault_ep.recv(());

  if let Some(info) = RootMessageLabel::try_from(&message) {
      match info {
          RootMessageLabel::RegisterIRQ(irq_handler, irq_num) => {
              BootInfo::irq_control()
                .irq_control_get(irq_num, &tasks[badge as usize].abs_cptr(irq_handler))
                .unwrap();

              // Reply message
              with_ipc_buffer_mut(|buffer| {
                  reply(buffer, MessageInfo::new(0, 0, 0, 0));
              });
          }
          ...
      }
  } else {
      let fault = with_ipc_buffer(|buffer| sel4::Fault::new(buffer, &message));
      debug_println!("fault {:#x?}", fault)
  }
}
```

在掌管着 irq_control 的任务中需要一个 Endpoint 与其他需要注册的任务相通，*请保证这些 EndPoint 是由同一个 Endpoint 派生出的。*这里与 FaultEP 共用，所以在使用时也可能接收到的时 Fault IPC，上述逻辑先判断消息是不是 RootMessageLabel 中列出的 IPC，如果是的话，就进行处理。主要逻辑在 irq_control() 处，利用 irq_control_get 生成一个 irq_handler 特定任务的一个 slot 中，这里的 Slot 使用的是绝对位置。需要保证当前 CSpace 中由这个任务的 CNode，使用上面我们提到的 crate 就可以像图中一样使用，不用考虑 CSpace 和 CNode 的问题。

之前提到过发送 IPC 的时候需要在 call 传递参数为一个 MessageInfo，为什么这里有些区别？因为在进行 IPC 的时候步骤比较麻烦，当想要通过 IPC 发送几个参数的时候，第一部就是利用 `with_ipc_buffer_mut` 闭包函数更改 IPCBuffer，将需要发送的参数写入到 IPCBuffer 中，然后再调用 MessageInfo::new 构造 MessageInfo，而且需要手动进行编号，这个过程十分繁琐，当任务和 IPC 的类型比较多时，对 MessageLabel 进行编址就是一项重复且繁琐的事情。可以将这个逻辑进行封装，调用时只需要像调用函数一样将参数和类型写入就行了，得益于 Rust 便捷的枚举类型，这个过程可以变得更加简单和优雅。下面给出 RootMessageLabel 的定义和相关的接口。

```rs
#[repr(usize)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RootMessageLabel {
    RegisterIRQ(CPtrBits, u64),
    TranslateAddr(usize),
}

impl RootMessageLabel {
    const LABEL_START: u64 = 0x200;

    /// Try to convert a MessageInfo to a RootMessageLabel
    pub fn try_from(message: &MessageInfo) -> Option<Self> {
        // Get the true index for the CustomMessageLabel
        let label = match message.label() >= Self::LABEL_START {
            true => message.label() - Self::LABEL_START,
            false => return None,
        };
        // Convert the true index to a RootMessageLabel enum
        with_ipc_buffer(|buffer| {
            let regs = buffer.msg_regs();
            match label {
                0x0 => Some(Self::RegisterIRQ(regs[0], regs[1])),
                0x1 => Some(Self::TranslateAddr(regs[0] as _)),
                _ => None,
            }
        })
    }

    pub fn to_label(&self) -> u64 {
        let n = match self {
            Self::RegisterIRQ(_, _) => 0,
            Self::TranslateAddr(_) => 1,
        };
        Self::LABEL_START + n
    }

    pub fn build(&self) -> MessageInfo {
        const REG_SIZE: usize = core::mem::size_of::<u64>();
        let caps_unwrapped = 0;
        let extra_caps = 0;
        let mut msg_size = 0;

        with_ipc_buffer_mut(|buffer| match self {
            RootMessageLabel::RegisterIRQ(irq_handler, irq_num) => {
                let regs = buffer.msg_regs_mut();
                regs[0] = *irq_handler;
                regs[1] = *irq_num;
                msg_size = 2 * REG_SIZE;
            }
            Self::TranslateAddr(addr) => {
                buffer.msg_regs_mut()[0] = *addr as _;
                msg_size = REG_SIZE;
            }
        });

        MessageInfo::new(self.to_label(), caps_unwrapped, extra_caps, msg_size)
    }
}
```

上面的函数依旧可以优化，找出他们的共通之初，然后形成一个 macro 协助统一编址和构建 IPCBuffer。

#bibliography("ref.yml")