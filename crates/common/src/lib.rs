#![no_std]

use sel4::MessageInfo;

/// Custom Message Label for transfer between tasks.
#[repr(usize)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CustomMessageLabel {
    TestCustomMessage = 0,
    Exit = 1,
}

impl CustomMessageLabel {
    /// The start index of the custom message label
    const LABEL_START: u64 = 0x100;

    /// Try to convert a MessageInfo to a CustomMessageLabel
    pub fn try_from(message: &MessageInfo) -> Option<CustomMessageLabel> {
        // Get the true index for the CustomMessageLabel
        let label = match message.label() > Self::LABEL_START {
            true => message.label() - Self::LABEL_START,
            false => return None,
        };
        // Convert the true index to a CustomMessageLabel enum
        match label {
            0x0 => Some(CustomMessageLabel::TestCustomMessage),
            0x1 => Some(CustomMessageLabel::Exit),
            _ => None,
        }
    }

    pub fn to_label(&self) -> u64 {
        *self as u64 + Self::LABEL_START
    }
}
