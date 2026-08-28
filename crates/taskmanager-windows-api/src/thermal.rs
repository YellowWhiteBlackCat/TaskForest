//! Audited native Windows ACPI thermal zone temperature querying via WMI/COM.

use crate::WindowsApiError;

const MAX_WMI_OBJECTS: usize = 256;

/// An ACPI thermal zone temperature reading in Celsius.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsThermalZoneReading {
    pub name: String,
    pub temperature_c: f32,
    pub critical_trip_point_c: Option<f32>,
}

/// Query ACPI thermal zones from root\wmi (MSAcpi_ThermalZoneTemperature).
#[must_use = "inspect thermal zone query result"]
pub fn query_acpi_thermal_zones() -> Result<Vec<WindowsThermalZoneReading>, WindowsApiError> {
    #[cfg(windows)]
    {
        query_acpi_thermal_zones_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_acpi_thermal_zones_windows() -> Result<Vec<WindowsThermalZoneReading>, WindowsApiError> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::System::Wmi::{IWbemLocator, WbemLocator};

    // SAFETY: COM initialization on current thread with multi-threaded apartment.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let needs_uninit = hr.is_ok();

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: CoUninitialize matches successful CoInitializeEx.
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }
    let _com_guard = ComGuard(needs_uninit);

    // SAFETY: CoCreateInstance for IWbemLocator with WbemLocator CLSID.
    let locator: IWbemLocator = unsafe {
        CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| WindowsApiError::QueryFailed)?
    };

    // 1. Try ROOT\WMI (MSAcpi_ThermalZoneTemperature)
    if let Ok(results) = query_wmi_thermal(
        &locator,
        "ROOT\\WMI",
        "SELECT InstanceName, CurrentTemperature, CriticalTripPoint FROM MSAcpi_ThermalZoneTemperature",
        "InstanceName",
        "CurrentTemperature",
        true,
    ) && !results.is_empty()
    {
        return Ok(results);
    }

    // 2. Try ROOT\CIMV2 (Win32_PerfFormattedData_Counters_ThermalZoneInformation)
    if let Ok(results) = query_wmi_thermal(
        &locator,
        "ROOT\\CIMV2",
        "SELECT Name, HighPrecisionTemperature, Temperature FROM Win32_PerfFormattedData_Counters_ThermalZoneInformation",
        "Name",
        "HighPrecisionTemperature",
        true,
    ) && !results.is_empty()
    {
        return Ok(results);
    }

    // 3. Try ROOT\CIMV2 (Win32_PerfFormattedData_Counters_ThermalZoneInformation with standard Temperature)
    if let Ok(results) = query_wmi_thermal(
        &locator,
        "ROOT\\CIMV2",
        "SELECT Name, Temperature FROM Win32_PerfFormattedData_Counters_ThermalZoneInformation",
        "Name",
        "Temperature",
        false,
    ) && !results.is_empty()
    {
        return Ok(results);
    }

    // 4. Try ROOT\LibreHardwareMonitor / OpenHardwareMonitor if present
    for ns in &["ROOT\\LibreHardwareMonitor", "ROOT\\OpenHardwareMonitor"] {
        if let Ok(results) = query_lhm_thermal(&locator, ns)
            && !results.is_empty()
        {
            return Ok(results);
        }
    }

    Err(WindowsApiError::QueryFailed)
}

#[cfg(windows)]
fn query_wmi_thermal(
    locator: &windows::Win32::System::Wmi::IWbemLocator,
    namespace_str: &str,
    query_str: &str,
    name_prop: &str,
    temp_prop: &str,
    is_tenths_kelvin: bool,
) -> Result<Vec<WindowsThermalZoneReading>, WindowsApiError> {
    use windows::Win32::System::Com::{
        CoSetProxyBlanket, EOLE_AUTHENTICATION_CAPABILITIES, RPC_C_AUTHN_LEVEL_CALL,
        RPC_C_IMP_LEVEL_IMPERSONATE,
    };
    use windows::Win32::System::Variant::{VARIANT, VT_BSTR, VT_I4, VariantClear};
    use windows::Win32::System::Wmi::{
        WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE,
    };
    use windows::core::BSTR;

    let namespace = BSTR::from(namespace_str);
    // SAFETY: ConnectServer to given namespace.
    let services = unsafe {
        locator
            .ConnectServer(
                &namespace,
                &BSTR::default(),
                &BSTR::default(),
                &BSTR::default(),
                0,
                &BSTR::default(),
                None,
            )
            .map_err(|_| WindowsApiError::QueryFailed)?
    };

    // SAFETY: Set proxy blanket on the IWbemServices interface.
    unsafe {
        let _ = CoSetProxyBlanket(
            &services,
            10,
            0,
            None,
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOLE_AUTHENTICATION_CAPABILITIES(0),
        );
    }

    let language = BSTR::from("WQL");
    let query = BSTR::from(query_str);
    let flags = WBEM_FLAG_FORWARD_ONLY.0 | WBEM_FLAG_RETURN_IMMEDIATELY.0;

    // SAFETY: ExecQuery returns enumerator for thermal zone instances.
    let enumerator = unsafe {
        services
            .ExecQuery(&language, &query, WBEM_GENERIC_FLAG_TYPE(flags), None)
            .map_err(|_| WindowsApiError::QueryFailed)?
    };

    let mut results = Vec::new();
    let mut objects = [const { None }; 8];
    let mut returned = 0u32;
    let mut seen_objects = 0usize;

    // SAFETY: Next retrieves thermal zone objects into the array.
    loop {
        // SAFETY: the enumerator is a live COM interface and `objects` plus
        // `returned` are writable outputs sized for eight VARIANT pointers.
        let status = unsafe { enumerator.Next(1000, &mut objects, &mut returned) };
        if status.is_err() {
            return Err(WindowsApiError::QueryFailed);
        }
        let returned = usize::try_from(returned).map_err(|_| WindowsApiError::ResourceLimit)?;
        if returned == 0 {
            break;
        }
        if returned > objects.len() || seen_objects > MAX_WMI_OBJECTS.saturating_sub(returned) {
            return Err(WindowsApiError::ResourceLimit);
        }
        seen_objects += returned;
        for obj_opt in &mut objects[..returned] {
            let Some(obj) = obj_opt.take() else { continue };

            let mut temp_val = VARIANT::default();
            let mut name_val = VARIANT::default();

            // SAFETY: obj is valid COM pointer, VARIANT cleared on exit.
            let temp_k = unsafe {
                let name = BSTR::from(temp_prop);
                if obj.Get(&name, 0, &mut temp_val, None, None).is_ok() {
                    let k = if temp_val.Anonymous.Anonymous.vt == VT_I4 {
                        Some(temp_val.Anonymous.Anonymous.Anonymous.lVal)
                    } else {
                        None
                    };
                    let _ = VariantClear(&mut temp_val);
                    k.unwrap_or_default()
                } else {
                    0
                }
            };

            // SAFETY: obj is valid COM pointer, VARIANT cleared on exit.
            let name_str = unsafe {
                let name = BSTR::from(name_prop);
                if obj.Get(&name, 0, &mut name_val, None, None).is_ok() {
                    let s = if name_val.Anonymous.Anonymous.vt == VT_BSTR {
                        let bstr = &name_val.Anonymous.Anonymous.Anonymous.bstrVal;
                        if !bstr.is_empty() {
                            bstr.to_string()
                        } else {
                            "Thermal Zone".to_string()
                        }
                    } else {
                        "Thermal Zone".to_string()
                    };
                    let _ = VariantClear(&mut name_val);
                    s
                } else {
                    "Thermal Zone".to_string()
                }
            };

            let temp_c = if is_tenths_kelvin {
                if (2732..=4032).contains(&temp_k) {
                    Some(((temp_k - 2732) as f32) / 10.0)
                } else {
                    None
                }
            } else if (273..=403).contains(&temp_k) {
                Some((temp_k - 273) as f32)
            } else {
                None
            };

            if let Some(temp_c) = temp_c {
                results.push(WindowsThermalZoneReading {
                    name: name_str,
                    temperature_c: temp_c,
                    critical_trip_point_c: None,
                });
            }
        }
    }

    Ok(results)
}

#[cfg(windows)]
fn query_lhm_thermal(
    locator: &windows::Win32::System::Wmi::IWbemLocator,
    namespace_str: &str,
) -> Result<Vec<WindowsThermalZoneReading>, WindowsApiError> {
    use windows::Win32::System::Com::{
        CoSetProxyBlanket, EOLE_AUTHENTICATION_CAPABILITIES, RPC_C_AUTHN_LEVEL_CALL,
        RPC_C_IMP_LEVEL_IMPERSONATE,
    };
    use windows::Win32::System::Variant::{VARIANT, VT_BSTR, VT_I4, VT_R4, VT_R8, VariantClear};
    use windows::Win32::System::Wmi::{
        WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE,
    };
    use windows::core::BSTR;

    let namespace = BSTR::from(namespace_str);
    // SAFETY: ConnectServer to given namespace.
    let services = unsafe {
        locator
            .ConnectServer(
                &namespace,
                &BSTR::default(),
                &BSTR::default(),
                &BSTR::default(),
                0,
                &BSTR::default(),
                None,
            )
            .map_err(|_| WindowsApiError::QueryFailed)?
    };

    // SAFETY: Set proxy blanket on the IWbemServices interface.
    unsafe {
        let _ = CoSetProxyBlanket(
            &services,
            10,
            0,
            None,
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOLE_AUTHENTICATION_CAPABILITIES(0),
        );
    }

    let language = BSTR::from("WQL");
    let query = BSTR::from("SELECT Name, Value FROM Sensor WHERE SensorType = 'Temperature'");
    let flags = WBEM_FLAG_FORWARD_ONLY.0 | WBEM_FLAG_RETURN_IMMEDIATELY.0;

    // SAFETY: ExecQuery returns enumerator for sensor instances.
    let enumerator = unsafe {
        services
            .ExecQuery(&language, &query, WBEM_GENERIC_FLAG_TYPE(flags), None)
            .map_err(|_| WindowsApiError::QueryFailed)?
    };

    let mut results = Vec::new();
    let mut objects = [const { None }; 8];
    let mut returned = 0u32;
    let mut seen_objects = 0usize;

    // SAFETY: Next retrieves sensor objects.
    loop {
        // SAFETY: the enumerator is a live COM interface and `objects` plus
        // `returned` are writable outputs sized for eight VARIANT pointers.
        let status = unsafe { enumerator.Next(1000, &mut objects, &mut returned) };
        if status.is_err() {
            return Err(WindowsApiError::QueryFailed);
        }
        let returned = usize::try_from(returned).map_err(|_| WindowsApiError::ResourceLimit)?;
        if returned == 0 {
            break;
        }
        if returned > objects.len() || seen_objects > MAX_WMI_OBJECTS.saturating_sub(returned) {
            return Err(WindowsApiError::ResourceLimit);
        }
        seen_objects += returned;
        for obj_opt in &mut objects[..returned] {
            let Some(obj) = obj_opt.take() else { continue };

            let mut val_var = VARIANT::default();
            let mut name_var = VARIANT::default();

            // SAFETY: obj is valid COM pointer, VARIANT cleared on exit.
            let val_f32 = unsafe {
                let name = BSTR::from("Value");
                if obj.Get(&name, 0, &mut val_var, None, None).is_ok() {
                    let v = match val_var.Anonymous.Anonymous.vt {
                        VT_R4 => val_var.Anonymous.Anonymous.Anonymous.fltVal,
                        VT_R8 => val_var.Anonymous.Anonymous.Anonymous.dblVal as f32,
                        VT_I4 => val_var.Anonymous.Anonymous.Anonymous.lVal as f32,
                        _ => 0.0,
                    };
                    let _ = VariantClear(&mut val_var);
                    v
                } else {
                    0.0
                }
            };

            // SAFETY: obj is valid COM pointer, VARIANT cleared on exit.
            let name_str = unsafe {
                let name = BSTR::from("Name");
                if obj.Get(&name, 0, &mut name_var, None, None).is_ok() {
                    let s = if name_var.Anonymous.Anonymous.vt == VT_BSTR {
                        let bstr = &name_var.Anonymous.Anonymous.Anonymous.bstrVal;
                        if !bstr.is_empty() {
                            bstr.to_string()
                        } else {
                            "CPU Temperature".to_string()
                        }
                    } else {
                        "CPU Temperature".to_string()
                    };
                    let _ = VariantClear(&mut name_var);
                    s
                } else {
                    "CPU Temperature".to_string()
                }
            };

            if val_f32 > 0.0 && val_f32 < 130.0 {
                results.push(WindowsThermalZoneReading {
                    name: name_str,
                    temperature_c: val_f32,
                    critical_trip_point_c: None,
                });
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_thermal.rs"]
mod tests;
