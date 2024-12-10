//! SmolTCP Impl module
//!
//! Implemente the trait that smoltcp needed in this module.
//! This file was inspird by virtio_drivers' example.

mod addr;
mod bench;
mod listen_table;
mod loopback;
mod tcp;
pub mod test;

use axdriver_net::{NetBufPtr, NetDriverOps};
use axdriver_virtio::{MmioTransport, VirtIoNetDev};
use axerrno::{AxError, AxResult};
use core::cell::RefCell;
use core::ops::DerefMut;
use core::sync::atomic::AtomicU64;
use lazyinit::LazyInit;
use listen_table::ListenTable;
use log::{debug, trace, warn};
use sel4::debug_println;

use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::{self, AnySocket, Socket};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr};
use spin::Mutex;
pub(crate) use tcp::*;
// Qemu IP
const IP: &str = "10.0.2.15";
const GATEWAY: &str = "10.0.2.2";
const IP_PREFIX: u8 = 24;

const DNS_SEVER: &str = "8.8.8.8";
const RANDOM_SEED: u64 = 0xA2CE_05A2_CE05_A2CE;
const STANDARD_MTU: usize = 1500;
const TCP_RX_BUF_LEN: usize = 64 * 64;
const TCP_TX_BUF_LEN: usize = 64 * 64;
const UDP_RX_BUF_LEN: usize = 64 * 64;
const UDP_TX_BUF_LEN: usize = 64 * 64;
const LISTEN_QUEUE_SIZE: usize = 32;

/// I/O poll results.
#[derive(Debug, Default, Clone, Copy)]
#[allow(unused)]
pub struct PollState {
    /// Object can be read now.
    pub readable: bool,
    /// Object can be writen now.
    pub writable: bool,
}

pub type NetDevice = VirtIoNetDev<VirtIoHalImpl, MmioTransport, 32>;

struct AxNetRxToken<'a>(&'a RefCell<NetDevice>, NetBufPtr);
struct AxNetTxToken<'a>(&'a RefCell<NetDevice>);

impl<'a> RxToken for AxNetRxToken<'a> {
    fn preprocess(&self, sockets: &mut SocketSet<'_>) {
        snoop_tcp_packet(self.1.packet(), sockets).ok();
    }

    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut rx_buf = self.1;
        trace!(
            "RECV {} bytes: {:02X?}",
            rx_buf.packet_len(),
            rx_buf.packet()
        );
        let result = f(rx_buf.packet_mut());
        self.0.borrow_mut().recycle_rx_buffer(rx_buf).unwrap();
        result
    }
}

impl<'a> TxToken for AxNetTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut dev = self.0.borrow_mut();
        let mut tx_buf = dev.alloc_tx_buffer(len).unwrap();
        let ret = f(tx_buf.packet_mut());
        trace!("SEND {} bytes: {:02X?}", len, tx_buf.packet());
        dev.transmit(tx_buf).unwrap();
        ret
    }
}

fn snoop_tcp_packet(buf: &[u8], sockets: &mut SocketSet<'_>) -> Result<(), smoltcp::wire::Error> {
    use smoltcp::wire::{EthernetFrame, IpProtocol, Ipv4Packet, TcpPacket};

    let ether_frame = EthernetFrame::new_checked(buf)?;
    let ipv4_packet = Ipv4Packet::new_checked(ether_frame.payload())?;

    if ipv4_packet.next_header() == IpProtocol::Tcp {
        let tcp_packet = TcpPacket::new_checked(ipv4_packet.payload())?;
        let src_addr = (ipv4_packet.src_addr(), tcp_packet.src_port()).into();
        let dst_addr = (ipv4_packet.dst_addr(), tcp_packet.dst_port()).into();
        let is_first = tcp_packet.syn() && !tcp_packet.ack();
        if is_first {
            // create a socket for the first incoming TCP packet, as the later accept() returns.
            LISTEN_TABLE.incoming_tcp_packet(src_addr, dst_addr, sockets);
        }
    }
    Ok(())
}

impl Device for DeviceWrapper {
    type RxToken<'a> = AxNetRxToken<'a> where Self: 'a;
    type TxToken<'a> = AxNetTxToken<'a> where Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut dev = self.inner.borrow_mut();
        if let Err(e) = dev.recycle_tx_buffers() {
            warn!("recycle_tx_buffers failed: {:?}", e);
            return None;
        }

        if !dev.can_transmit() {
            return None;
        }
        let rx_buf = match dev.receive() {
            Ok(buf) => buf,
            Err(_err) => {
                return None;
            }
        };
        Some((AxNetRxToken(&self.inner, rx_buf), AxNetTxToken(&self.inner)))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        let mut dev = self.inner.borrow_mut();
        if let Err(e) = dev.recycle_tx_buffers() {
            warn!("recycle_tx_buffers failed: {:?}", e);
            return None;
        }
        if dev.can_transmit() {
            Some(AxNetTxToken(&self.inner))
        } else {
            None
        }
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1514;
        caps.max_burst_size = None;
        caps.medium = Medium::Ethernet;
        caps
    }
}

static LISTEN_TABLE: LazyInit<ListenTable> = LazyInit::new();
static SOCKET_SET: LazyInit<SocketSetWrapper> = LazyInit::new();

#[allow(unused)]
static LOOPBACK_DEV: LazyInit<Mutex<LoopbackDev>> = LazyInit::new();
#[allow(unused)]
static LOOPBACK: LazyInit<Mutex<Interface>> = LazyInit::new();
use crate::virtio_impl::VirtIoHalImpl;

use self::loopback::LoopbackDev;

static ETH0: LazyInit<InterfaceWrapper> = LazyInit::new();

struct SocketSetWrapper<'a>(Mutex<SocketSet<'a>>);

struct DeviceWrapper {
    inner: RefCell<NetDevice>, // use `RefCell` is enough since it's wrapped in `Mutex` in `InterfaceWrapper`.
}

pub(crate) struct InterfaceWrapper {
    name: &'static str,
    ether_addr: EthernetAddress,
    dev: Mutex<DeviceWrapper>,
    iface: Mutex<Interface>,
}

#[allow(unused)]
impl<'a> SocketSetWrapper<'a> {
    fn new() -> Self {
        Self(Mutex::new(SocketSet::new(vec![])))
    }

    pub fn new_tcp_socket() -> socket::tcp::Socket<'a> {
        let tcp_rx_buffer = socket::tcp::SocketBuffer::new(vec![0; TCP_RX_BUF_LEN]);
        let tcp_tx_buffer = socket::tcp::SocketBuffer::new(vec![0; TCP_TX_BUF_LEN]);
        socket::tcp::Socket::new(tcp_rx_buffer, tcp_tx_buffer)
    }

    pub fn new_udp_socket() -> socket::udp::Socket<'a> {
        let udp_rx_buffer = socket::udp::PacketBuffer::new(
            vec![socket::udp::PacketMetadata::EMPTY; 256],
            vec![0; UDP_RX_BUF_LEN],
        );
        let udp_tx_buffer = socket::udp::PacketBuffer::new(
            vec![socket::udp::PacketMetadata::EMPTY; 256],
            vec![0; UDP_TX_BUF_LEN],
        );
        socket::udp::Socket::new(udp_rx_buffer, udp_tx_buffer)
    }

    pub fn new_dns_socket() -> socket::dns::Socket<'a> {
        let server_addr = DNS_SEVER.parse().expect("invalid DNS server address");
        socket::dns::Socket::new(&[server_addr], vec![])
    }

    pub fn add<T: AnySocket<'a>>(&self, socket: T) -> SocketHandle {
        let handle = self.0.lock().add(socket);
        debug!("socket {}: created", handle);

        handle
    }

    pub fn with_socket<T: AnySocket<'a>, R, F>(&self, handle: SocketHandle, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let set = self.0.lock();
        let socket = set.get(handle);
        f(socket)
    }

    pub fn with_socket_mut<T: AnySocket<'a>, R, F>(&self, handle: SocketHandle, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let mut set = self.0.lock();
        let socket = set.get_mut(handle);
        f(socket)
    }

    pub fn bind_check(&self, addr: IpAddress, _port: u16) -> AxResult {
        let mut sockets = self.0.lock();
        for item in sockets.iter_mut() {
            match item.1 {
                Socket::Tcp(s) => {
                    let local_addr = s.get_bound_endpoint();
                    if local_addr.addr == Some(addr) {
                        return Err(AxError::AddrInUse);
                    }
                }
                Socket::Udp(s) => {
                    if s.endpoint().addr == Some(addr) {
                        return Err(AxError::AddrInUse);
                    }
                }
                _ => continue,
            };
        }
        Ok(())
    }

    pub fn poll_interfaces(&self) {
        LOOPBACK.lock().poll(
            InterfaceWrapper::current_time(),
            LOOPBACK_DEV.lock().deref_mut(),
            &mut self.0.lock(),
        );

        ETH0.poll(&self.0);
    }

    pub fn remove(&self, handle: SocketHandle) {
        self.0.lock().remove(handle);
        debug!("socket {}: destroyed", handle);
    }
}
static TIME_TICKS: AtomicU64 = AtomicU64::new(0);
#[allow(unused)]
impl InterfaceWrapper {
    fn new(name: &'static str, dev: NetDevice, ether_addr: EthernetAddress) -> Self {
        let mut config = Config::new(HardwareAddress::Ethernet(ether_addr));
        config.random_seed = RANDOM_SEED;

        let mut dev = DeviceWrapper::new(dev);
        let iface = Mutex::new(Interface::new(config, &mut dev, Self::current_time()));
        Self {
            name,
            ether_addr,
            dev: Mutex::new(dev),
            iface,
        }
    }

    pub fn current_time() -> Instant {
        TIME_TICKS.fetch_add(1, core::sync::atomic::Ordering::Release);
        Instant::from_micros(TIME_TICKS.load(core::sync::atomic::Ordering::Acquire) as i64)
    }

    pub fn name(&self) -> &str {
        self.name
    }

    pub fn ethernet_address(&self) -> EthernetAddress {
        self.ether_addr
    }

    pub fn setup_ip_addr(&self, ip: IpAddress, prefix_len: u8) {
        let mut iface = self.iface.lock();
        iface.update_ip_addrs(|ip_addrs| {
            ip_addrs.push(IpCidr::new(ip, prefix_len)).unwrap();
        });
    }

    pub fn setup_gateway(&self, gateway: IpAddress) {
        let mut iface = self.iface.lock();
        match gateway {
            IpAddress::Ipv4(v4) => iface.routes_mut().add_default_ipv4_route(v4).unwrap(),
            IpAddress::Ipv6(v6) => iface.routes_mut().add_default_ipv6_route(v6).unwrap(),
        };
    }

    pub fn poll(&self, sockets: &Mutex<SocketSet>) {
        let mut dev = self.dev.lock();
        let mut iface = self.iface.lock();
        let mut sockets = sockets.lock();
        let timestamp = Self::current_time();
        iface.poll(timestamp, dev.deref_mut(), &mut sockets);
    }
}

impl DeviceWrapper {
    fn new(inner: NetDevice) -> Self {
        Self {
            inner: RefCell::new(inner),
        }
    }
}

/// Poll the network stack.
///
/// It may receive packets from the NIC and process them, and transmit queued
/// packets to the NIC.
#[allow(unused)]
pub fn poll_interfaces() {
    SOCKET_SET.poll_interfaces();
}

/// Benchmark raw socket transmit bandwidth.
#[allow(unused)]
pub fn bench_transmit() {
    ETH0.dev.lock().bench_transmit_bandwidth();
}

/// Benchmark raw socket receive bandwidth.
#[allow(unused)]
pub fn bench_receive() {
    ETH0.dev.lock().bench_receive_bandwidth();
}

pub(crate) fn init(net_dev: NetDevice) {
    // A fake address
    let ether_addr = EthernetAddress([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let eth0 = InterfaceWrapper::new("eth0", net_dev, ether_addr);
    debug_println!("[Net thread] Create Interface!");
    let ip = IP.parse().unwrap();
    let gateway = GATEWAY.parse().unwrap();

    eth0.setup_ip_addr(ip, IP_PREFIX);
    eth0.setup_gateway(gateway);

    ETH0.init_once(eth0);
    debug_println!("[Net thread] Init the ethrnet device!");
    SOCKET_SET.init_once(SocketSetWrapper::new());
    debug_println!("[Net thread] Init the socket set!");
    LISTEN_TABLE.init_once(ListenTable::new());

    let mut loopback_device = LoopbackDev::new(Medium::Ip);
    let config = Config::new(smoltcp::wire::HardwareAddress::Ip);

    let mut iface = Interface::new(
        config,
        &mut loopback_device,
        InterfaceWrapper::current_time(),
    );

    iface.update_ip_addrs(|ipaddr| {
        ipaddr
            .push(IpCidr::new(IpAddress::v4(127, 0, 0, 1), 8))
            .unwrap()
    });

    LOOPBACK.init_once(Mutex::new(iface));
    LOOPBACK_DEV.init_once(Mutex::new(loopback_device));
    debug_println!("[Net thread] Init the loopback device!");
}
