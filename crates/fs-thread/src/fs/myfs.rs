use crate::dev::Disk;
use alloc::sync::Arc;
use axfs_ramfs::RamFileSystem;
use axfs_vfs::VfsOps;

/// The interface to define custom filesystems in user apps.
#[crate_interface::def_interface]
pub trait MyFileSystemIf {
    /// Creates a new instance of the filesystem with initialization.
    ///
    /// TODO: use generic disk type
    fn new_myfs(disk: Disk) -> Arc<dyn VfsOps>;
}

pub(crate) fn new_myfs(disk: Disk) -> Arc<dyn VfsOps> {
    crate_interface::call_interface!(MyFileSystemIf::new_myfs(disk))
}

struct MyFileSystemIfImpl;

#[crate_interface::impl_interface]
impl MyFileSystemIf for MyFileSystemIfImpl {
    fn new_myfs(_disk: Disk) -> Arc<dyn VfsOps> {
        Arc::new(RamFileSystem::new())
    }
}
