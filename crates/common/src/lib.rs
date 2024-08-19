#![no_std]

use core::cell::UnsafeCell;

use sel4::{MessageInfo, GRANULE_SIZE};

/// Custom Message Label for transfer between tasks.
#[repr(usize)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CustomMessageLabel {
    TestCustomMessage = 0,
    SysCall = 1,
    Exit = 2,
}

impl CustomMessageLabel {
    /// The start index of the custom message label
    const LABEL_START: u64 = 0x100;

    /// Try to convert a MessageInfo to a CustomMessageLabel
    pub fn try_from(message: &MessageInfo) -> Option<CustomMessageLabel> {
        // Get the true index for the CustomMessageLabel
        let label = match message.label() >= Self::LABEL_START {
            true => message.label() - Self::LABEL_START,
            false => return None,
        };
        // Convert the true index to a CustomMessageLabel enum
        match label {
            0x0 => Some(CustomMessageLabel::TestCustomMessage),
            0x1 => Some(CustomMessageLabel::SysCall),
            0x2 => Some(CustomMessageLabel::Exit),
            _ => None,
        }
    }

    pub fn to_label(&self) -> u64 {
        *self as u64 + Self::LABEL_START
    }
}

/// Page aligned with [GRANULE_SIZE]
#[repr(align(4096))]
pub struct AlignedPage(UnsafeCell<[u8; GRANULE_SIZE.bytes()]>);

impl AlignedPage {
    /// Create a new aligned page with [GRANULE_SIZE] of data
    pub const fn new() -> Self {
        Self(UnsafeCell::new([0; GRANULE_SIZE.bytes()]))
    }

    /// Get the ptr of the aligned page
    pub const fn ptr(&self) -> *mut u8 {
        self.0.get() as _
    }
}
