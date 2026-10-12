#![forbid(unsafe_code)]

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!(
        "taskmanager-smart-self-test-helper is the Linux pkexec/storage helper; \
         there is no build of its operation on this platform"
    );
}

#[cfg(target_os = "linux")]
#[path = "smart_self_test.rs"]
mod smart_self_test;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    smart_self_test::run()
}
