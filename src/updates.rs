//! Product identity and release policy; implementation lives in rust-auto-updator.
pub type Update = Box<rust_auto_updator::Release>;

pub fn config() -> Result<rust_auto_updator::Config, String> {
    let mut config = rust_auto_updator::Config::new(
        "siarheisinelnikau",
        "magic-language-switcher",
        env!("CARGO_PKG_VERSION"),
        "MagicLanguageSwitcher",
    )?;
    config.asset_suffixes = vec!["-x64".into(), "-windows-x64".into()];
    config.expected_subsystem = Some(2);
    config.install.stage_prefix = ".MagicLanguageSwitcher-shared-update-".into();
    config.install.helper_name = "helper.exe".into();
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(suffix: &str) -> String {
        let filename = format!("MagicLanguageSwitcher-v99.0.0{suffix}.exe");
        serde_json::json!({
            "tag_name": "v99.0.0", "draft": false, "prerelease": false,
            "html_url": "https://github.com/siarheisinelnikau/magic-language-switcher/releases/tag/v99.0.0",
            "assets": [
                {"name": filename, "browser_download_url": format!("https://github.com/siarheisinelnikau/magic-language-switcher/releases/download/v99.0.0/{filename}"), "size": 4096, "state": "uploaded", "digest": format!("sha256:{}", "ab".repeat(32))},
                {"name": "SHA256SUMS.txt", "browser_download_url": "https://github.com/siarheisinelnikau/magic-language-switcher/releases/download/v99.0.0/SHA256SUMS.txt", "size": 100, "state": "uploaded"}
            ]
        }).to_string()
    }

    #[test]
    fn keeps_historical_x64_filenames_without_accepting_other_architectures() {
        for suffix in ["", "-x64", "-windows-x64"] {
            assert!(
                rust_auto_updator::parse_release(&release(suffix), &config().unwrap())
                    .unwrap()
                    .is_some()
            );
        }
        for suffix in ["-x86", "-arm64"] {
            assert!(
                rust_auto_updator::parse_release(&release(suffix), &config().unwrap()).is_err()
            );
        }
    }

    #[test]
    fn rejects_a_foreign_project_download() {
        let json = release("").replace("/releases/download/", "/foreign/download/");
        assert!(rust_auto_updator::parse_release(&json, &config().unwrap()).is_err());
    }
}
