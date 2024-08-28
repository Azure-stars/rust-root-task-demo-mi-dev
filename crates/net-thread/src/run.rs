use core::{net::Ipv4Addr, pin::pin, str::FromStr};

use alloc::sync::Arc;
use sel4::debug_println;
use sel4_async_single_threaded_executor::run_until_stalled;
use smoltcp::{
    iface::{Config, Interface, SocketSet},
    socket::tcp,
    time::Instant,
    wire::{HardwareAddress, IpAddress, IpCidr, Ipv4Cidr},
};
use spin::Mutex;
use virtio_drivers::{device::net::VirtIONet, transport::mmio::MmioTransport};

use crate::{smoltcp_impl::NetDevice, virtio_impl::HalImpl};

/// Rust Async runtime entry
///
/// Jump to async function from general function.
pub fn async_runtime_entry(net_dev: VirtIONet<HalImpl, MmioTransport, 32>) {
    let afn = pin!(run_server(net_dev));
    let _ = run_until_stalled(afn);
}

/// Async runtime entry.
///
/// Boot a http server and display basic information.
pub async fn run_server(net_dev: VirtIONet<HalImpl, MmioTransport, 32>) {
    debug_println!("Hello Test Run Server function");
    log::info!("Test Server: {}", "NET THREAD");

    let mut net_dev = NetDevice(Arc::new(Mutex::new(net_dev)));
    let mac_address = net_dev.0.lock().mac_address();
    let mut config = Config::new(HardwareAddress::Ethernet(smoltcp::wire::EthernetAddress(
        mac_address,
    )));
    config.random_seed = 0x12345;
    let mut iface = Interface::new(config, &mut net_dev, Instant::from_secs(0));
    iface.update_ip_addrs(|ip_addrs| {
        ip_addrs
            .push(IpCidr::new(
                IpAddress::Ipv4(smoltcp::wire::Ipv4Address::from_str("10.0.2.15").unwrap()),
                24,
            ))
            .expect("can't update ip address");
    });
    iface
        .routes_mut()
        .add_default_ipv4_route(smoltcp::wire::Ipv4Address::from_str("10.0.2.2").unwrap())
        .expect("can't add router gateway");

    let tcp_rx_buffer = tcp::SocketBuffer::new(vec![0; 1024]);
    let tcp_tx_buffer = tcp::SocketBuffer::new(vec![0; 1024]);
    let tcp_socket = tcp::Socket::new(tcp_rx_buffer, tcp_tx_buffer);

    let mut socks = SocketSet::new(vec![]);
    let tcp_handle = socks.add(tcp_socket);
    log::info!("Ready to poll socket");

    const PORT: u16 = 6379;
    let mut tcp_active = false;
    loop {
        iface.poll(Instant::from_secs(1), &mut net_dev, &mut socks);
        let socket = socks.get_mut::<tcp::Socket>(tcp_handle);
        if !socket.is_open() {
            log::info!("listening on port {}", PORT);
            socket.listen(PORT).unwrap();
        }

        if socket.is_active() && !tcp_active {
            log::info!("tcp {PORT} connected");
        } else if !socket.is_active() && tcp_active {
            log::info!("tcp {PORT} disconnected");
        }
        tcp_active = socket.is_active();
        // log::info!("receive a tcp socket information");
    }
}
