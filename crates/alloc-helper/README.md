# Alloc Helper

Alloc Helper 可以快速支持 rust alloc crate.
用法：

在需要使用的 crate 中使用以下代码使用。

```rust
/// 默认 allocator 内存大小
const DEFAULT_ALLOCATOR_SIZE: usize = 0x8000;
define_allocator! {
    /// 定义一个 global_allocator
    /// 名称为 GLOBAL_ALLOCATOR
    /// 内存大小为 [DEFAULT_ALLOCATOR_SIZE]
    (GLOBAL_ALLOCATOR, DEFAULT_ALLOCATOR_SIZE)
}
```

Alloc Helper 提供了一个宏 define_allocator, 可以根据这个结构快速定义支持 rust alloc crate 的支持。

DEFAULT_ALLOCATOR_SIZE 定义了默认的 alloc crate 大小。在 define_allocator 宏中指定的 GLOBAL_ALLOCATOR 为生成的 global allocator 名称。

本 crate 基于 buddy_system_allocator 实现。
