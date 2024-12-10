use core::net::{Ipv4Addr, SocketAddr};

use alloc::vec::Vec;
use axerrno::AxError;
use common::LibcSocketAddr;
use memory_addr::{MemoryAddr, VirtAddr, PAGE_SIZE_4K};
use sel4::{init_thread, Cap, CapRights, VmAttributes};
use syscalls::Errno;

use crate::{page_seat_vaddr, syscall::SysResult, task::Sel4Task, utils::align_bits};

use super::ipc::tcp;

fn process_item_list<T: Sized, F>(
    task: &mut Sel4Task,
    addr: VirtAddr,
    number: Option<usize>,
    mut f: F,
) -> usize
where
    F: FnMut(VirtAddr, usize, usize),
{
    let mut buf_addr = addr;
    if number.is_none() && core::mem::size_of::<T>() + addr.align_offset_4k() > PAGE_SIZE_4K {
        panic!("Item size is too large");
    }
    let number = number.unwrap_or(1);
    let mut len = core::mem::size_of::<T>() * number;
    while len > 0 {
        if let Some(cap) = task.mapped_page.get(&align_bits(buf_addr.as_usize(), 12)) {
            let new_cap = Cap::<sel4::cap_type::SmallPage>::from_bits(0);
            init_thread::slot::CNODE
                .cap()
                .relative(new_cap)
                .copy(
                    &init_thread::slot::CNODE.cap().relative(*cap),
                    CapRights::all(),
                )
                .unwrap();

            new_cap
                .frame_map(
                    init_thread::slot::VSPACE.cap(),
                    page_seat_vaddr(),
                    CapRights::all(),
                    VmAttributes::DEFAULT,
                )
                .unwrap();
            let copy_len = (PAGE_SIZE_4K - buf_addr.align_offset_4k()).min(len);
            f(
                VirtAddr::from_usize(page_seat_vaddr() + buf_addr.align_offset_4k()),
                buf_addr - addr,
                copy_len,
            );
            len -= copy_len;
            buf_addr += copy_len;

            new_cap.frame_unmap().unwrap();
            init_thread::slot::CNODE
                .cap()
                .relative(new_cap)
                .delete()
                .unwrap();
        } else {
            break;
        }
    }
    buf_addr - addr
}

fn read_item<T: Sized + Copy + Default>(task: &mut Sel4Task, addr: *const T) -> T {
    let mut item: T = T::default();
    process_item_list::<T, _>(task, VirtAddr::from_ptr_of(addr), None, |src, _, _| {
        item = unsafe { core::ptr::read_volatile(src.as_ptr() as *const T) }
    });
    item
}

fn write_item<T: Sized + Copy>(task: &mut Sel4Task, addr: *const T, item: &T) {
    process_item_list::<T, _>(
        task,
        VirtAddr::from_ptr_of(addr),
        None,
        |dst, _, copy_len| unsafe {
            core::ptr::copy_nonoverlapping(item as *const T, dst.as_mut_ptr() as *mut T, copy_len);
        },
    );
}

fn read_item_list<T: Sized + Copy>(
    task: &mut Sel4Task,
    addr: *const T,
    num: Option<usize>,
    buf: &mut [T],
) {
    process_item_list::<T, _>(
        task,
        VirtAddr::from_ptr_of(addr),
        num,
        |src, offset, copy_len| {
            // let bytes =
            //     unsafe { core::slice::from_raw_parts(buf_addr.as_ptr(), offset + copy_len) };
            unsafe {
                core::ptr::copy_nonoverlapping(
                    src.as_ptr() as *const T,
                    buf.as_mut_ptr().add(offset),
                    copy_len,
                );
            }
        },
    );
}

/// Write items to the given address.
///
/// # Arguments
///
/// - buf: The buffer to write.
/// - addr: The address to write to.
fn write_item_list<T: Sized + Copy>(
    task: &mut Sel4Task,
    addr: *mut T,
    num: Option<usize>,
    buf: &[T],
) -> usize {
    process_item_list::<u8, _>(
        task,
        VirtAddr::from_ptr_of(addr),
        num,
        |dst, offset, copy_len| unsafe {
            core::ptr::copy_nonoverlapping(
                buf.as_ptr().add(offset),
                dst.as_mut_ptr() as *mut T,
                copy_len,
            );
        },
    )
}

pub fn sys_socket(
    _task: &mut Sel4Task,
    _domain: usize,
    _type: usize,
    _protocol: usize,
) -> SysResult {
    Ok(tcp::new() as usize)
}

pub fn sys_bind(
    task: &mut Sel4Task,
    socket_fd: i32,
    addr: *const LibcSocketAddr,
    _addr_len: u32,
) -> SysResult {
    let addr = read_item(task, addr);
    let socket_id = socket_fd as u64;
    let local_addr: SocketAddr = addr.into();
    match tcp::bind(socket_id, local_addr) {
        Ok(()) => Ok(0),
        Err(AxError::InvalidInput) => Err(Errno::EINVAL),
        Err(_) => panic!("Unknown Error!"),
    }
}

pub fn sys_connect(
    task: &mut Sel4Task,
    socket_fd: i32,
    addr: *const LibcSocketAddr,
    _addr_len: u32,
) -> SysResult {
    let addr = read_item(task, addr);
    let socket_id = socket_fd as u64;
    let remote_addr: SocketAddr = addr.into();
    match tcp::connect(socket_id, remote_addr) {
        Ok(()) => Ok(0),
        Err(AxError::InvalidInput) | Err(AxError::AddrInUse) => Err(Errno::EINVAL),
        Err(_) => panic!("Unknown Error!"),
    }
}

pub fn sys_listen(_task: &mut Sel4Task, socket_fd: i32) -> SysResult {
    let socket_id = socket_fd as u64;
    match tcp::listen(socket_id) {
        Ok(()) => Ok(0),
        Err(AxError::InvalidInput) | Err(AxError::AddrInUse) => Err(Errno::EINVAL),
        Err(_) => panic!("Unknown Error!"),
    }
}

pub fn sys_accept(
    task: &mut Sel4Task,
    socket_fd: i32,
    addr: *mut LibcSocketAddr,
    _addr_len: u32,
) -> SysResult {
    fn parse_ipaddr(is_ipv4: bool, addr_low: u64, addr_high: u64, port: u16) -> SocketAddr {
        if is_ipv4 {
            let addr = addr_low.to_be_bytes();
            SocketAddr::new(
                core::net::IpAddr::V4(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3])),
                port,
            )
        } else {
            let addr = (addr_high as u128) << 32 | addr_low as u128;
            let addr = addr.to_be_bytes();
            SocketAddr::new(core::net::IpAddr::V6(addr.into()), port)
        }
    }
    let socket_id = socket_fd as u64;
    match tcp::accept(socket_id) {
        Ok(ans) => {
            let socket_id = ans[0] as usize;
            let is_ipv4 = ans[1] != 0;
            let port = ans[2] as u16;
            let socket_addr = parse_ipaddr(is_ipv4, ans[3], ans[4], port);
            write_item(task, addr, &socket_addr.into());
            Ok(socket_id)
        }
        Err(AxError::InvalidInput) | Err(AxError::AddrInUse) => Err(Errno::EINVAL),
        Err(_) => panic!("Unknown Error!"),
    }
}

pub fn sys_shutdown(_task: &mut Sel4Task, socket_fd: i32, _how: i32) -> SysResult {
    let socket_id = socket_fd as u64;
    match tcp::shutdown(socket_id) {
        Ok(()) => Ok(0),
        Err(AxError::InvalidInput) | Err(AxError::AddrInUse) => Err(Errno::EINVAL),
        Err(_) => panic!("Unknown Error!"),
    }
}

pub fn sys_sendto(
    task: &mut Sel4Task,
    socket_fd: i32,
    buf: *const u8,
    len: usize,
    _flags: i32,
    _addr: *const LibcSocketAddr,
    _addr_len: usize,
) -> SysResult {
    let socket_id = socket_fd as u64;
    let _remote_addr: SocketAddr = read_item(task, _addr).into();
    // TODO: copy the capabilities of the user thread and transmit it directly
    let mut payload = Vec::with_capacity(len);
    read_item_list(task, buf, Some(len), payload.as_mut_slice());
    match tcp::send(socket_id, payload.as_slice()) {
        Ok(len) => Ok(len),
        Err(AxError::InvalidInput) | Err(AxError::AddrInUse) => Err(Errno::EINVAL),
        Err(_) => panic!("Unknown Error!"),
    }
}

pub fn sys_recvfrom(
    task: &mut Sel4Task,
    socket_fd: i32,
    buf: *mut u8,
    len: usize,
    _flags: i32,
    _addr: *const LibcSocketAddr,
    _addr_len: usize,
) -> SysResult {
    let socket_id = socket_fd as u64;
    let mut recv_buf = Vec::with_capacity(len);
    match tcp::recv(socket_id, recv_buf.as_mut_slice()) {
        Ok(len) => Ok(write_item_list(task, buf, Some(len), recv_buf.as_slice())),
        Err(AxError::InvalidInput) | Err(AxError::AddrInUse) => Err(Errno::EINVAL),
        Err(_) => panic!("Unknown Error!"),
    }
}
