#![no_std]

use sel4::Endpoint;

/// The default notification for thread lock.
pub const DEFAULT_THREAD_NOTIFICATION: u64 = 17;
/// The default endpoint for thread lock.
pub const DEFAULT_THREAD_FAULT_EP: u64 = 18;
/// The default slot to store thread recv cap.
pub const DEFAULT_THREAD_RECV_SLOT: u64 = 19;
/// The default endpoint for thread IRQ.
pub const DEFAULT_THREAD_IRQ_EP: u64 = 20;
/// The default slot to store custom cap.
pub const DEFAULT_CUSTOM_SLOT: u64 = 26;
/// The Default Index of the empty slot.
pub const DEFAULT_EMPTY_SLOT_INDEX: usize = 32;

// Init End point, used in tasks.
pub const INIT_EP: Endpoint = Endpoint::from_bits(DEFAULT_THREAD_FAULT_EP);

// CNode Bits
pub const DEFAULT_CNODE_BITS: u64 = 12;
pub const DEFAULT_CNODE_SLOT_NUMS: usize = 1 << DEFAULT_CNODE_BITS;
