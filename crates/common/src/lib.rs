#![no_std]

use core::cell::UnsafeCell;

use sel4::{with_ipc_buffer, with_ipc_buffer_mut, CPtrBits, MessageInfo};

// FIXME: Make this variable more generic.
pub const VIRTIO_MMIO_ADDR: usize = 0xa003e00;

pub const PAGE_SIZE: usize = sel4::FrameObjectType::GRANULE.bytes();

/// Impl custom message label quickly.
macro_rules! impl_message_label {
    {
        $(#[$m:meta])*
        pub enum $name:ident : $start:literal {
            $(
                $field:ident $(( $($t:ty),* ))? => $num:literal
            ),* $(,)?
        }
    } => {
        $(#[$m])*
        pub enum $name {
            $($field $(($($t),*))? ),*
        }

        impl $name {

            pub fn try_from(raw_label: usize) -> Option<Self> {
                let label = raw_label - $start;
                match label {
                    $(
                        $num => {
                            todo!()
                        }
                    )*
                    _ => None
                }
            }
        }
    }
}

impl_message_label! {
    #[repr(usize)]
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub enum TestMessageLabel: 100 {
        MessageLabel => 0,
        Test1 => 1,
        Test2(u8,u8) => 2
    }
}

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

pub type IrqNum = u64;

#[repr(usize)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RootMessageLabel {
    RegisterIRQ(CPtrBits, IrqNum),
    TranslateAddr(usize),
    RegisterIRQWithCap(IrqNum),
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
                0x0 => Some(Self::RegisterIRQ(regs[0], regs[1] as _)),
                0x1 => Some(Self::TranslateAddr(regs[0] as _)),
                0x2 => Some(Self::RegisterIRQWithCap(regs[0] as _)),
                _ => None,
            }
        })
    }

    pub fn to_label(&self) -> u64 {
        let n = match self {
            RootMessageLabel::RegisterIRQ(_, _) => 0,
            RootMessageLabel::TranslateAddr(_) => 1,
            RootMessageLabel::RegisterIRQWithCap(_) => 2,
        };
        Self::LABEL_START + n
    }

    pub fn build(&self) -> MessageInfo {
        let caps_unwrapped = 0;
        let extra_caps = 0;
        let mut msg_size = 0;

        with_ipc_buffer_mut(|buffer| {
            let regs = buffer.msg_regs_mut();
            match self {
                RootMessageLabel::RegisterIRQ(irq_handler, irq_num) => {
                    regs[0] = *irq_handler;
                    regs[1] = *irq_num;
                    msg_size = 2;
                }
                RootMessageLabel::TranslateAddr(addr) => {
                    regs[0] = *addr as _;
                    msg_size = 1;
                }
                RootMessageLabel::RegisterIRQWithCap(irq_num) => {
                    regs[0] = *irq_num;
                    msg_size = 1;
                }
            }
        });

        MessageInfo::new(self.to_label(), caps_unwrapped, extra_caps, msg_size)
    }
}

#[repr(usize)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlkMessageLabel {
    Ping,
    ReadBlock(u64, u64),
    WriteBlock(u64, u64),
    NumBlock,
}

impl BlkMessageLabel {
    const LABEL_START: u64 = 0x300;

    /// Try to convert a MessageInfo to a BlkMessageLabel
    pub fn try_from(message: &MessageInfo) -> Option<Self> {
        // Get the true index for the BlkMessageLabel
        let label = match message.label() >= Self::LABEL_START {
            true => message.label() - Self::LABEL_START,
            false => return None,
        };
        // Convert the true index to a BlkMessageLabel enum
        with_ipc_buffer(|buffer| {
            let regs = buffer.msg_regs();
            match label {
                0x0 => Some(Self::Ping),
                0x1 => Some(Self::ReadBlock(regs[0], regs[1])),
                0x2 => Some(Self::WriteBlock(regs[0], regs[1])),
                0x3 => Some(Self::NumBlock),
                _ => None,
            }
        })
    }

    pub fn to_label(&self) -> u64 {
        let n = match self {
            Self::Ping => 0,
            Self::ReadBlock(_, _) => 1,
            Self::WriteBlock(_, _) => 2,
            Self::NumBlock => 3,
        };
        Self::LABEL_START + n
    }

    pub fn build(&self) -> MessageInfo {
        let caps_unwrapped = 0;
        let extra_caps = 0;
        let mut msg_size = 0;

        with_ipc_buffer_mut(|buffer| match self {
            Self::Ping => {}
            Self::ReadBlock(idx, num) => {
                let regs = buffer.msg_regs_mut();
                regs[0] = *idx;
                regs[1] = *num;
                msg_size = 2;
            }
            Self::WriteBlock(idx, num) => {
                let regs = buffer.msg_regs_mut();
                regs[0] = *idx;
                regs[1] = *num;
                msg_size = 2;
            }
            Self::NumBlock => {}
        });

        MessageInfo::new(self.to_label(), caps_unwrapped, extra_caps, msg_size)
    }
}

/// Page aligned with [GRANULE_SIZE]
#[repr(align(4096))]
pub struct AlignedPage(UnsafeCell<[u8; PAGE_SIZE]>);

impl AlignedPage {
    /// Create a new aligned page with [GRANULE_SIZE] of data
    pub const fn new() -> Self {
        Self(UnsafeCell::new([0; PAGE_SIZE]))
    }

    /// Get the ptr of the aligned page
    pub const fn ptr(&self) -> *mut u8 {
        self.0.get() as _
    }
}
