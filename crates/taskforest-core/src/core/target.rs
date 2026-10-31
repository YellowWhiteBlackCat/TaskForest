//! Opaque targets carried across application and native-provider boundaries.
//!
//! These values deliberately do not validate one operating system's syntax.
//! They prevent semantically unrelated strings from being interchanged while
//! allowing each native adapter to resolve its own target vocabulary.

use std::borrow::Borrow;
use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! opaque_target {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            #[must_use]
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
    };
}

opaque_target!(
    /// Provider-neutral, provider-issued service target.
    ///
    /// Frontends compare and return this opaque identity unchanged. They never
    /// derive it from a display name or reinterpret its provider-native payload.
    ServiceId
);
opaque_target!(
    /// Provider-neutral login-session target.
    SessionId
);
opaque_target!(
    /// Opaque native storage locator used to execute or poll a SMART job.
    ///
    /// This is intentionally distinct from [`super::DeviceId`]: a locator may
    /// be a transient device path while `DeviceId` identifies lifecycle.
    StorageDeviceKey
);

#[cfg(test)]
#[path = "../../tests/headless/core_core_target_tests.rs"]
mod tests;
