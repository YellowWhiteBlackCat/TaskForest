//! Integration providers: command launch, resource reveal, URL open, and
//! desktop appearance.

use super::*;

impl CommandLaunchProvider for FakeProvider {
    fn run_command(&mut self, _command: &str) -> Result<u32, ProviderFailure> {
        thread::sleep(self.delay);
        Ok(9001)
    }
}

impl ResourceRevealProvider for FakeProvider {
    fn reveal_process(
        &mut self,
        target: &FrozenProcessIdentity,
        _cached_executable: Option<&std::path::Path>,
    ) -> Result<(), ProviderFailure> {
        self.revealed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(target.clone());
        Ok(())
    }
}

impl UrlOpenProvider for FakeProvider {
    fn open_url(&mut self, _url: &str) -> Result<(), ProviderFailure> {
        Ok(())
    }
}

impl DesktopAppearanceProvider for FakeProvider {
    fn observe(&mut self) -> Result<CompositeSourceSnapshot<DesktopAppearance>, ProviderFailure> {
        Ok(CompositeSourceSnapshot::new(
            DesktopAppearance::default(),
            vec![fixture_source(
                "fixture.desktop-appearance",
                1,
                self.observation_source_failure,
            )],
        ))
    }
}
