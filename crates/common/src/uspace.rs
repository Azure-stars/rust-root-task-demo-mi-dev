//! It defines all kinds of configuration about user space.

use core::net::{Ipv4Addr, SocketAddr};

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

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct LibcSocketAddr {
    pub sa_family: u16,
    pub sa_data: [u8; 14usize],
}

impl From<SocketAddr> for LibcSocketAddr {
    fn from(value: SocketAddr) -> Self {
        let mut addr = LibcSocketAddr::default();
        // FIXME: It default use AF_INET domain. And it use ipv4 address.
        addr.sa_family = 2;
        let ip = value.ip();
        match ip {
            core::net::IpAddr::V4(ip) => {
                let ip = ip.octets();
                for i in 0..4 {
                    addr.sa_data[i + 2] = ip[i];
                }
            }
            core::net::IpAddr::V6(ip) => {
                let ip = ip.octets();
                for i in 0..12 {
                    addr.sa_data[i + 2] = ip[i];
                }
            }
        }
        addr.sa_data[0] = (value.port() >> 8) as u8;
        addr.sa_data[1] = (value.port() & 0xff) as u8;
        addr
    }
}

impl Into<SocketAddr> for LibcSocketAddr {
    fn into(self) -> SocketAddr {
        // FIXME: It default use AF_INET domain. And it use ipv4 address.
        let data = self.sa_data;
        let port = u16::from_be_bytes([data[0], data[1]]);

        SocketAddr::new(
            core::net::IpAddr::V4(Ipv4Addr::new(data[2], data[3], data[4], data[5])),
            port,
        )
    }
}
