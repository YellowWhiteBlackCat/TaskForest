use super::*;
use taskmanager_core::core::identity::DeviceId;

#[test]
fn suspending_gpu_pacing_retains_the_session_binding_for_resume() {
    let binding = GpuEngineBinding {
        index: 0,
        device_id: DeviceId::new("gpu:resume"),
    };
    let mut pacing = GpuEnginePacingState::default();
    let first_generation = pacing.start(binding.clone());
    assert!(pacing.is_polling(first_generation));

    assert!(pacing.stop(false));
    assert!(!pacing.is_polling(first_generation));
    assert_eq!(pacing.binding(), Some(&binding));

    let resumed_generation = pacing.resume().expect("bound pacing must resume");
    assert_ne!(resumed_generation, first_generation);
    assert!(pacing.is_polling(resumed_generation));
}
