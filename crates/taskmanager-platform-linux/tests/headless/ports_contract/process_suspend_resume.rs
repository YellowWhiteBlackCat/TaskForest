//! Neutral Suspend/Resume control requests stay concept-first: the Linux
//! composition edge maps them onto the Unix stop/continue signal primitives
//! and completion rides the existing signal event.

use super::*;
use taskmanager_core::ProcessSignal;

#[test]
fn suspend_and_resume_map_to_stop_and_continue_signal_completions() {
    let signaled = Arc::new(Mutex::new(Vec::new()));
    let handle = spawn_complete(fake_registry(FakeProvider {
        signaled: signaled.clone(),
        ..Default::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let target = FrozenProcessIdentity::from_authoritative_parts(77, "control-lane", 500, 5_000)
        .expect("fixture identity");

    let suspend_id = submit_process_control(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_CONTROL,
        ProcessControlRequest::Suspend {
            target: target.clone(),
        },
    );
    let event = wait_event(&handle);
    assert_eq!(event.request_id, suspend_id);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Processes(ProcessEvent::SignalCompleted {
            signal: ProcessSignal::Stop,
            ..
        }))
    ));

    let resume_id = submit_process_control(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_CONTROL,
        ProcessControlRequest::Resume { target },
    );
    let event = wait_event(&handle);
    assert_eq!(event.request_id, resume_id);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Processes(ProcessEvent::SignalCompleted {
            signal: ProcessSignal::Continue,
            ..
        }))
    ));

    assert_eq!(
        signaled.lock().expect("signaled pairs").as_slice(),
        &[(77, ProcessSignal::Stop), (77, ProcessSignal::Continue)]
    );
}
