use super::*;

#[test]
fn smbios_firmware_query_behavior() {
    let info = query_firmware_info();
    let mem = query_memory_hardware_info();
    eprintln!("LIVE FIRMWARE INFO: {info:?}");
    eprintln!("LIVE SMBIOS MEMORY: {mem:?}");
}
