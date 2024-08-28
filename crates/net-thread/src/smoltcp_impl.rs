//! SmolTCP Impl module
//!
//! Implemente the trait that smoltcp needed in this module.
//! This file was inspird by virtio_drivers' example.
use alloc::sync::Arc;
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use spin::Mutex;
use virtio_drivers::{
    device::net::{RxBuffer, VirtIONet},
    transport::mmio::MmioTransport,
};

use crate::virtio_impl::HalImpl;

type VNetDev = VirtIONet<HalImpl, MmioTransport, 32>;
pub struct NetDevice(pub Arc<Mutex<VNetDev>>);
pub struct TXToken(pub Arc<Mutex<VNetDev>>);
pub struct RXToken(pub Arc<Mutex<VNetDev>>, RxBuffer);

impl TxToken for TXToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut dev = self.0.lock();
        let mut tx_buffer = dev.new_tx_buffer(len);
        let result = f(tx_buffer.packet_mut());
        dev.send(tx_buffer).unwrap();
        result
    }
}

impl RxToken for RXToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut rx_buffer = self.1;
        let result = f(rx_buffer.packet_mut());
        self.0
            .lock()
            .recycle_rx_buffer(rx_buffer)
            .expect("Can'f recycle rx buffer");
        result
    }
}

impl Device for NetDevice {
    type RxToken<'a> = RXToken;

    type TxToken<'a> = TXToken;

    fn receive(
        &mut self,
        _timestamp: smoltcp::time::Instant,
    ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        match self.0.lock().receive() {
            Ok(buf) => Some((RXToken(self.0.clone(), buf), TXToken(self.0.clone()))),
            Err(virtio_drivers::Error::NotReady) => None,
            Err(err) => {
                log::debug!("error: {err:#x?}");
                todo!()
            }
        }
    }

    fn transmit(&mut self, _timertamp: smoltcp::time::Instant) -> Option<Self::TxToken<'_>> {
        Some(TXToken(self.0.clone()))
    }

    fn capabilities(&self) -> smoltcp::phy::DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1536;
        caps.max_burst_size = Some(1);
        caps.medium = Medium::Ethernet;
        caps
    }
}
