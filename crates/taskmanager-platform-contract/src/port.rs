use crate::{CapabilityRequest, EventEnvelope, EventPortError, RequestEnvelope, SubmissionError};

/// Non-blocking command side of one independently composable capability facet.
///
/// Implementations must not perform filesystem, process, device, or command
/// work in this method. They may only enqueue into a bounded runtime.
///
/// A request type must own one static capability association. This makes it
/// impossible for runtime composition to implement a typed port using an
/// unclassified request:
///
/// ```compile_fail
/// use taskmanager_platform_contract::{
///     RequestEnvelope, RequestPort, SubmissionError,
/// };
///
/// struct UnclassifiedRequest;
/// struct InvalidPort;
///
/// impl RequestPort for InvalidPort {
///     type Request = UnclassifiedRequest;
///
///     fn try_submit(
///         &self,
///         _request: RequestEnvelope<Self::Request>,
///     ) -> Result<(), SubmissionError> {
///         Ok(())
///     }
/// }
/// ```
pub trait RequestPort: Send + Sync {
    type Request: CapabilityRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError>;
}

/// Non-blocking event side of one independently composable capability facet.
pub trait EventPort: Send + Sync {
    type Event: Send + 'static;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError>;
}
