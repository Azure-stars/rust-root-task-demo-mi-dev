#![no_std]

use core::cell::UnsafeCell;

use sel4::{with_ipc_buffer, with_ipc_buffer_mut, CPtrBits, MessageInfo, GRANULE_SIZE};

// FIXME: Make this variable more generic.
pub const VIRTIO_MMIO_ADDR: usize = 0xa003e00;

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
    pub fn try_from(message: &MessageInfo) -> Option<Self> {
        // Get the true index for the CustomMessageLabel
        let label = match message.label() >= Self::LABEL_START {
            true => message.label() - Self::LABEL_START,
            false => return None,
        };
        // Convert the true index to a CustomMessageLabel enum
        match label {
            0x0 => Some(Self::TestCustomMessage),
            0x1 => Some(Self::SysCall),
            0x2 => Some(Self::Exit),
            _ => None,
        }
    }

    pub fn to_label(&self) -> u64 {
        *self as u64 + Self::LABEL_START
    }
}

#[repr(usize)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RootMessageLabel {
    RegisterIRQ(CPtrBits, u64),
    TranslateAddr(usize),
}

impl RootMessageLabel {
    const LABEL_START: u64 = 0x200;

    /// Try to convert a MessageInfo to a RootMessageLabel
    pub fn try_from(message: &MessageInfo) -> Option<Self> {
        // Get the true index for the CustomMessageLabel
        let label = match message.label() >= Self::LABEL_START {
            true => message.label() - Self::LABEL_START,
            false => return None,
        };
        // Convert the true index to a RootMessageLabel enum
        with_ipc_buffer(|buffer| {
            let regs = buffer.msg_regs();
            match label {
                0x0 => Some(Self::RegisterIRQ(regs[0], regs[1])),
                0x1 => Some(Self::TranslateAddr(regs[0] as _)),
                _ => None,
            }
        })
    }

    pub fn to_label(&self) -> u64 {
        let n = match self {
            Self::RegisterIRQ(_, _) => 0,
            Self::TranslateAddr(_) => 1,
        };
        Self::LABEL_START + n
    }

    pub fn build(&self) -> MessageInfo {
        const REG_SIZE: usize = core::mem::size_of::<u64>();
        let caps_unwrapped = 0;
        let extra_caps = 0;
        let mut msg_size = 0;

        with_ipc_buffer_mut(|buffer| match self {
            RootMessageLabel::RegisterIRQ(irq_handler, irq_num) => {
                let regs = buffer.msg_regs_mut();
                regs[0] = *irq_handler;
                regs[1] = *irq_num;
                msg_size = 2 * REG_SIZE;
            }
            Self::TranslateAddr(addr) => {
                buffer.msg_regs_mut()[0] = *addr as _;
                msg_size = REG_SIZE;
            }
        });

        MessageInfo::new(self.to_label(), caps_unwrapped, extra_caps, msg_size)
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
