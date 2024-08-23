# 怎么在 RootTask 创建一个任务

可以参考 [Task Helper Crate](../../crates/task-helper/README.md)

## 创建 cap 

```rust
    let cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let inner_cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let tcb = alloc_cap::<cap_type::TCB>();
    let vspace = alloc_cap::<cap_type::VSpace>();
```

这里会创建相应的 cap, CNODE_RADIX_BITS 的指为 12，这里由两个 cnode，一共为两级，一级为 2^12 = 4096 个 slot，两级为 4096 * 4096 个 slot。

## 构建两级 CSpace

```rust
    // Build 2 level CSpace.
    // | unused (40 bits) | Level1 (12 bits) | Level0 (12 bits) |
    cnode
        .relative_bits_with_depth(0, CNODE_RADIX_BITS)
        .mutate(
            &abs_cptr(inner_cnode),
            CNodeCapData::skip(0).into_word() as _,
        )
        .unwrap();
    abs_cptr(BootInfo::null())
        .mutate(
            &abs_cptr(cnode),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word() as _,
        )
        .unwrap();
    abs_cptr(cnode)
        .mutate(
            &abs_cptr(BootInfo::null()),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word() as _,
        )
        .unwrap();
```

## 为任务 task 的 vspace 分配 asid

```rust
BootInfo::init_thread_asid_pool()
        .asid_pool_assign(vspace)
        .unwrap();
```

## 创建任务

```rust
    let mut task = Sel4Task::new(tcb, cnode, fault_ep.0, vspace, fault_ep.1);
```

## 配置任务

```rust
    // Configure Root Task
    task.configure(2 * CNODE_RADIX_BITS)?;

    // Map stack for the task.
    task.map_stack(10);

    // set task priority and max control priority
    task.tcb
        .tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 255, 255)?;
```

## 映射任务

```rust
// Map elf file for the task.
    task.map_elf(elf_file);
```

## 运行任务

```rust
    task.tcb.tcb_resume().unwrap();
```

## 完整代码

```rust
    let cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let inner_cnode = alloc_cap_size::<cap_type::CNode>(CNODE_RADIX_BITS);
    let tcb = alloc_cap::<cap_type::TCB>();
    let vspace = alloc_cap::<cap_type::VSpace>();

    // Build 2 level CSpace.
    // | unused (40 bits) | Level1 (12 bits) | Level0 (12 bits) |
    cnode
        .relative_bits_with_depth(0, CNODE_RADIX_BITS)
        .mutate(
            &abs_cptr(inner_cnode),
            CNodeCapData::skip(0).into_word() as _,
        )
        .unwrap();
    abs_cptr(BootInfo::null())
        .mutate(
            &abs_cptr(cnode),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word() as _,
        )
        .unwrap();
    abs_cptr(cnode)
        .mutate(
            &abs_cptr(BootInfo::null()),
            CNodeCapData::skip_high_bits(2 * CNODE_RADIX_BITS).into_word() as _,
        )
        .unwrap();

    BootInfo::init_thread_asid_pool()
        .asid_pool_assign(vspace)
        .unwrap();

    let mut task = Sel4Task::new(tcb, cnode, fault_ep.0, vspace, fault_ep.1);

    // Configure Root Task
    task.configure(2 * CNODE_RADIX_BITS)?;

    // Map stack for the task.
    task.map_stack(10);

    // set task priority and max control priority
    task.tcb
        .tcb_set_sched_params(sel4::BootInfo::init_thread_tcb(), 255, 255)?;

    // Map elf file for the task.
    task.map_elf(elf_file);

    task.tcb.tcb_resume().unwrap();
```

## 如何创建一个 线程？

在 sel4 中没有非常明显的线程和进程的区别，在需要创建一个线程的时候直接将一个已有的进程的资源 (CNode, VSpace, fault_ep) 复用即可，只需要一个新的 tcb。
