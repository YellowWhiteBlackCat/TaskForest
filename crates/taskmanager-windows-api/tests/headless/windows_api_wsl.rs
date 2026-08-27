use super::*;

#[test]
fn live_wsl_distributions_query() {
    let result = query_wsl_distributions();
    #[cfg(windows)]
    {
        assert!(result.is_ok());
        if let Ok(distros) = result {
            eprintln!("LIVE WSL DISTROS: {distros:?}");
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
