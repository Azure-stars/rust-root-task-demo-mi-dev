//! It defines all kinds of configuration about user space.

pub const USPACE_HEAP_BASE: usize = 0x1_0000_0000;
pub const USPACE_HEAP_SIZE: usize = 0x10_0000;

/// The highest address of the user space stack
pub const USPACE_STACK_TOP: usize = 0x2_0000_0000;
/// The maximum size of the user space stack
pub const USPACE_STACK_SIZE: usize = 0x1_0000;

/// The file descriptor for stdin
pub const STDIN_FD: i32 = 0;
/// The file descriptor for stdout
pub const STDOUT_FD: i32 = 1;
/// The file descriptor for stderr
pub const STDERR_FD: i32 = 2;

/// The lowest address of the user space
pub const USPACE_BASE: usize = 0x1000;

/// A void pointer in C
pub type CVoidPtr = usize;
