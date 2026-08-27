//! Emit a redaction-safe live-host capability receipt for target-machine runs.
//!
//! This receipt proves only that hardware/tool/backend markers were observed;
//! it must be paired with provider results before claiming ATA, NVML, or OpenRC
//! execution evidence.

use std::process::ExitCode;

use taskmanager_platform_linux::{
    collect_linux_provider_capability_receipt, linux_provider_capability_receipt_json,
};

fn main() -> ExitCode {
    let receipt = collect_linux_provider_capability_receipt();
    match linux_provider_capability_receipt_json(&receipt) {
        Ok(json) => {
            print!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not serialize Linux provider capability receipt: {error}");
            ExitCode::FAILURE
        }
    }
}
