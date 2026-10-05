//! Strict Wayland text clipboard. No X11 fallback or local success buffer.

use crate::ClipboardError;
use alloc::borrow::Cow;
use std::io::Read;
use wl_clipboard_rs::{copy, paste};

pub(crate) struct WaylandClipboard;

impl WaylandClipboard {
    pub(crate) fn get_text(&mut self) -> Result<String, ClipboardError> {
        let (mut pipe, _) = paste::get_contents(
            paste::ClipboardType::Regular,
            paste::Seat::Unspecified,
            paste::MimeType::Text,
        )
        .map_err(|error| match error {
            paste::Error::ClipboardEmpty | paste::Error::NoMimeType => {
                ClipboardError::ContentNotAvailable
            }
            paste::Error::MissingProtocol { .. } | paste::Error::NoSeats => {
                ClipboardError::ClipboardNotSupported
            }
            other => ClipboardError::Unknown {
                description: other.to_string(),
            },
        })?;
        let mut text = String::new();
        pipe.read_to_string(&mut text)
            .map_err(|_| ClipboardError::ConversionFailure)?;
        Ok(text)
    }

    pub(crate) fn set_text<'a, T: Into<Cow<'a, str>>>(
        &mut self,
        text: T,
    ) -> Result<(), ClipboardError> {
        // copy() returns only after the compositor accepted the data source.
        // The library keeps the selection source alive on its serving thread.
        copy::copy(
            copy::Options::new(),
            copy::Source::Bytes(text.into().into_owned().into_bytes().into()),
            copy::MimeType::Text,
        )
        .map_err(|error| ClipboardError::Unknown {
            description: error.to_string(),
        })
    }
}
