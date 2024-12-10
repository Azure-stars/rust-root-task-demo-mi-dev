//! The IPC module for the network thread.
//!
//! It will expose its interface by handling the IPC message from the kernel thread.
//!
//! Related IPC messages are defined in the [`common::NetRequsetabel`].

use alloc::vec::Vec;
use axerrno::AxResult;
use common::NetRequsetabel;
use core::net::{IpAddr, SocketAddr};
use crate_consts::DEFAULT_CUSTOM_SLOT;
use lazyinit::LazyInit;
use memory_addr::{MemoryAddr, VirtAddr, PAGE_SIZE_4K};
use sel4::{
    cap::Endpoint, debug_println, init_thread, reply, with_ipc_buffer_mut, Cap, CapRights,
    MessageInfo, VmAttributes,
};
use spin::Mutex;

use crate::smoltcp_impl::{self, *};

/// Reply a message with empty message information
///
/// Tips: It does not reply the message with any capability by default.
#[inline]
fn reply_with(regs: &[u64]) {
    with_ipc_buffer_mut(|buffer| {
        let msg_regs = buffer.msg_regs_mut();
        regs.iter()
            .enumerate()
            .for_each(|(i, reg)| msg_regs[i] = *reg as _);
        reply(buffer, MessageInfo::new(0, 0, 0, 8 * regs.len()))
    });
}

static SOCKET_VEC: LazyInit<Mutex<Vec<Option<TcpSocket>>>> = LazyInit::new();

fn alloc_socket_id() -> u64 {
    let mut sockets = SOCKET_VEC.lock();
    sockets.iter().position(|x| x.is_none()).unwrap_or_else(|| {
        sockets.push(None);
        sockets.len() - 1
    }) as u64
}

/// Get the virtual address of the page seat.
fn page_seat_vaddr() -> usize {
    0x1_0000_2000
}

/// Read an item from the given pointer.
///
/// # Arguments
/// * `ptr` - The pointer to the item.
/// * `cap` - The capability of the page which the `ptr` points to.
fn read_item<T: Sized + Copy>(ptr: *const T, cap: Cap<sel4::cap_type::SmallPage>) -> T {
    let buf_addr = VirtAddr::from_ptr_of(ptr);

    cap.frame_map(
        init_thread::slot::VSPACE.cap(),
        page_seat_vaddr(),
        CapRights::all(),
        VmAttributes::DEFAULT,
    )
    .unwrap();

    if PAGE_SIZE_4K - buf_addr.align_offset_4k() < core::mem::size_of::<T>() {
        panic!("The item is not contained in a page.");
    }
    let item =
        unsafe { ((page_seat_vaddr() + buf_addr.align_offset_4k()) as *const T).read_volatile() };
    cap.frame_unmap().unwrap();
    item
}

pub(crate) fn handle_ipc(recv_ep: &Endpoint) {
    fn handle_axresult(res: AxResult) -> u64 {
        match res {
            Ok(_) => 0,
            Err(e) => e.code() as u64,
        }
    }

    let new_cap = Cap::<sel4::cap_type::SmallPage>::from_bits(DEFAULT_CUSTOM_SLOT + 3);
    with_ipc_buffer_mut(|buf| {
        buf.set_recv_slot(&sel4::init_thread::slot::CNODE.cap().relative(new_cap));
    });

    let (message, _bade) = recv_ep.recv(());
    if message.label() < 0x8 {
        // Handle fault
        unimplemented!();
    } else {
        match NetRequsetabel::try_from(&message) {
            Some(NetRequsetabel::New) => {
                let socket = smoltcp_impl::TcpSocket::new();
                let id = alloc_socket_id();
                SOCKET_VEC.lock()[id as usize] = Some(socket);
                reply_with(&[id]);
            }
            Some(NetRequsetabel::IsNonBlocking(id)) => {
                let socket_vec = SOCKET_VEC.lock();
                let ans = socket_vec[id as usize]
                    .as_ref()
                    .map_or(0, |socket| socket.is_nonblocking() as i32);
                reply_with(&[ans as u64]);
            }
            Some(NetRequsetabel::Bind(id, local_addr)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].as_mut().unwrap();
                let local_addr = read_item(local_addr as *const SocketAddr, new_cap);
                let ans = socket.bind(local_addr);
                reply_with(&[handle_axresult(ans)]);
                new_cap.frame_unmap().unwrap();
            }
            Some(NetRequsetabel::Send(id, buf, buf_len)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].as_mut().unwrap();

                new_cap
                    .frame_map(
                        init_thread::slot::VSPACE.cap(),
                        page_seat_vaddr(),
                        CapRights::all(),
                        VmAttributes::DEFAULT,
                    )
                    .unwrap();
                if VirtAddr::from(buf as usize).align_offset_4k() + buf_len as usize > PAGE_SIZE_4K
                {
                    panic!("The buffer is not contained in a page.");
                }
                let buf = unsafe {
                    core::slice::from_raw_parts(page_seat_vaddr() as *const u8, buf_len as usize)
                };
                let ans = socket.send(buf);
                reply_with(&[ans.unwrap_or_else(|err| err.code() as usize) as u64]);

                new_cap.frame_unmap().unwrap();
            }

            Some(NetRequsetabel::Recv(id, buf, buf_len)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].as_mut().unwrap();
                new_cap
                    .frame_map(
                        init_thread::slot::VSPACE.cap(),
                        page_seat_vaddr(),
                        CapRights::all(),
                        VmAttributes::DEFAULT,
                    )
                    .unwrap();
                if VirtAddr::from(buf as usize).align_offset_4k() + buf_len as usize > PAGE_SIZE_4K
                {
                    panic!("The buffer is not contained in a page.");
                }
                let buf = unsafe {
                    core::slice::from_raw_parts_mut(page_seat_vaddr() as *mut u8, buf_len as usize)
                };
                let ans = socket.recv(buf);
                reply_with(&[ans.unwrap_or_else(|err| err.code() as usize) as u64]);

                new_cap.frame_unmap().unwrap();
            }
            Some(NetRequsetabel::RecvTimeout(id, buf, buf_len, timeout)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].as_mut().unwrap();

                new_cap
                    .frame_map(
                        init_thread::slot::VSPACE.cap(),
                        page_seat_vaddr(),
                        CapRights::all(),
                        VmAttributes::DEFAULT,
                    )
                    .unwrap();
                if VirtAddr::from(buf as usize).align_offset_4k() + buf_len as usize > PAGE_SIZE_4K
                {
                    panic!("The buffer is not contained in a page.");
                }
                let buf = unsafe {
                    core::slice::from_raw_parts_mut(page_seat_vaddr() as *mut u8, buf_len as usize)
                };
                let ans = socket.recv_timeout(buf, timeout);
                reply_with(&[ans.unwrap_or_else(|err| err.code() as usize) as u64]);

                new_cap.frame_unmap().unwrap();
            }
            Some(NetRequsetabel::Connect(id, remote_addr)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].as_mut().unwrap();
                let remote_addr = read_item(remote_addr as *const SocketAddr, new_cap);
                let ans = socket.connect(remote_addr);

                reply_with(&[handle_axresult(ans)]);
            }
            Some(NetRequsetabel::Listen(id)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].as_mut().unwrap();
                let ans = socket.listen();

                reply_with(&[handle_axresult(ans)]);
            }
            Some(NetRequsetabel::Accept(id)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].as_mut().unwrap();
                let ans = socket
                    .accept()
                    .map(|new_socket| {
                        let new_id = alloc_socket_id();
                        let socket_addr = new_socket.local_addr().unwrap();
                        // 将 IpAddr 的类型转换为 u64 类型
                        let (addr_low, addr_high) = match socket_addr.ip() {
                            IpAddr::V4(ipv4) => (ipv4.to_bits() as u64, 0),
                            IpAddr::V6(ipv6) => {
                                let addr: u128 = ipv6.to_bits();
                                (addr as u64, (addr >> 32) as u64)
                            }
                        };
                        socket_vec[new_id as usize] = Some(new_socket);
                        [
                            new_id,
                            socket_addr.is_ipv4() as u64,
                            socket_addr.port() as u64,
                            addr_low,
                            addr_high,
                        ]
                    })
                    .unwrap_or_else(|err| [err.code() as u64, 0, 0, 0, 0]);

                reply_with(&ans);
            }
            Some(NetRequsetabel::Close(id)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].take().unwrap();
                socket.close();
                reply_with(&[]);
            }

            Some(NetRequsetabel::SetNonBlocking(id, is_nonblocking)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].as_mut().unwrap();
                socket.set_nonblocking(is_nonblocking != 0);
                reply_with(&[]);
            }
            Some(NetRequsetabel::Shutdown(id)) => {
                let mut socket_vec = SOCKET_VEC.lock();
                let socket = socket_vec[id as usize].as_mut().unwrap();
                let ans = socket.shutdown();

                reply_with(&[handle_axresult(ans)]);
            }
            None => {
                debug_println!(
                    "[Net Thread] Recv unknown {} length message {:#x?} ",
                    message.length(),
                    message
                );
            }
        }
    }
    sel4::init_thread::slot::CNODE
        .cap()
        .relative(new_cap)
        .delete()
        .unwrap();
}

/// Initialize IPC
///
/// It will apply for an endpoint to communicate with the kernel thread.
#[allow(unused)]
pub(crate) fn run_ipc() {
    SOCKET_VEC.init_once(Mutex::new(Vec::new()));

    let ipc_ep = Cap::<sel4::cap_type::Endpoint>::from_bits(DEFAULT_CUSTOM_SLOT + 2);

    loop {
        handle_ipc(&ipc_ep);
    }
}
