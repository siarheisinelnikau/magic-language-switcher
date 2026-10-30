//! Stable version comparison and validation of this repository's release assets.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fmt, fs::File, io::Read, path::Path};

pub const REPOSITORY: &str = "siarheisinelnikau/magic-language-switcher";
pub const LATEST_API: &str =
    "https://api.github.com/repos/siarheisinelnikau/magic-language-switcher/releases/latest";
pub const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);
impl Version {
    pub fn parse(value: &str) -> Result<Self, String> {
        let parts: Vec<_> = value
            .strip_prefix('v')
            .unwrap_or(value)
            .split('.')
            .collect();
        if parts.len() != 3
            || parts.iter().any(|part| {
                part.is_empty()
                    || !part.bytes().all(|byte| byte.is_ascii_digit())
                    || part.len() > 1 && part.starts_with('0')
            })
        {
            return Err("Invalid stable release version.".into());
        }
        let number = |part: &str| {
            part.parse::<u32>()
                .map_err(|_| "Invalid stable release version.".to_owned())
        };
        Ok(Self(
            number(parts[0])?,
            number(parts[1])?,
            number(parts[2])?,
        ))
    }
}
impl fmt::Display for Version {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[derive(Clone, Debug)]
pub struct Update {
    pub version: Version,
    pub filename: String,
    pub url: String,
    pub checksum_url: String,
    pub sha256: String,
    pub size: u64,
}
impl Update {
    pub fn release_url(&self) -> String {
        format!(
            "https://github.com/{REPOSITORY}/releases/tag/v{}",
            self.version
        )
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
    state: String,
}

pub fn find_update(json: &[u8], current: Version) -> Result<Option<Update>, String> {
    let release: Release =
        serde_json::from_slice(json).map_err(|_| "GitHub returned an invalid release response.")?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(&release.tag_name)?;
    if version <= current {
        return Ok(None);
    }
    // Prefer the standalone filename; accept an explicit x64 variant and historical
    // names so development builds can still read already published releases.
    let filenames = [
        format!("MagicLanguageSwitcher-v{version}.exe"),
        format!("MagicLanguageSwitcher-v{version}-x64.exe"),
        format!("MagicLanguageSwitcher-v{version}-windows-x64.exe"),
    ];
    let asset = filenames
        .iter()
        .find_map(|filename| {
            release
                .assets
                .iter()
                .find(|asset| asset.name == *filename && asset.state == "uploaded")
        })
        .ok_or("This release has no compatible Windows x64 EXE yet.")?;
    let filename = asset.name.clone();
    let checksum = release
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS.txt" && asset.state == "uploaded")
        .ok_or("This release has no checksum file yet.")?;
    let prefix = format!("https://github.com/{REPOSITORY}/releases/download/v{version}/");
    if asset.browser_download_url != format!("{prefix}{filename}")
        || checksum.browser_download_url != format!("{prefix}SHA256SUMS.txt")
    {
        return Err("The release download does not belong to this project.".into());
    }
    if asset.size == 0 || asset.size > MAX_DOWNLOAD || checksum.size > 65536 {
        return Err("Invalid update download size.".into());
    }
    let digest = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|digest| valid_digest(digest))
        .ok_or("The release is missing a valid SHA-256 digest.")?;
    Ok(Some(Update {
        version,
        filename,
        url: asset.browser_download_url.clone(),
        checksum_url: checksum.browser_download_url.clone(),
        sha256: digest.to_ascii_lowercase(),
        size: asset.size,
    }))
}

pub fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
pub fn verify_checksum_file(contents: &[u8], update: &Update) -> Result<(), String> {
    let text = std::str::from_utf8(contents).map_err(|_| "Invalid checksum file.")?;
    let matching: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            (parts.len() == 2 && parts[1].trim_start_matches('*') == update.filename)
                .then(|| parts[0])
        })
        .collect();
    if matching.len() != 1 || !matching[0].eq_ignore_ascii_case(&update.sha256) {
        return Err("Release checksums do not match. The update was not installed.".into());
    }
    Ok(())
}
pub fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let length = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if length == 0 {
            break;
        }
        hash.update(&buffer[..length]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(version: &str) -> Vec<u8> {
        let filename = format!("MagicLanguageSwitcher-{version}.exe");
        serde_json::to_vec(&serde_json::json!({"tag_name":version,"draft":false,"prerelease":false,"assets":[
            {"name":filename,"browser_download_url":format!("https://github.com/{REPOSITORY}/releases/download/{version}/{filename}"),"size":400000,"digest":format!("sha256:{}", "ab".repeat(32)),"state":"uploaded"},
            {"name":"SHA256SUMS.txt","browser_download_url":format!("https://github.com/{REPOSITORY}/releases/download/{version}/SHA256SUMS.txt"),"size":111,"digest":null,"state":"uploaded"}
        ]})).unwrap()
    }
    #[test]
    fn compares_versions_numerically_and_never_downgrades() {
        assert!(Version::parse("v0.1.10").unwrap() > Version::parse("0.1.9").unwrap());
        assert!(
            find_update(&release("v0.1.1"), Version(0, 1, 2))
                .unwrap()
                .is_none()
        );
        assert!(
            find_update(&release("v0.1.2"), Version(0, 1, 2))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            find_update(&release("v0.1.3"), Version(0, 1, 2))
                .unwrap()
                .unwrap()
                .version,
            Version(0, 1, 3)
        );
        for invalid in ["1.2", "1.2.3-beta", "01.2.3", "1.2.3/evil", "-1.2.3"] {
            assert!(Version::parse(invalid).is_err());
        }
    }
    #[test]
    fn rejects_foreign_downloads_and_missing_integrity_data() {
        let original = release("v0.1.3");
        for change in ["url", "digest", "checksum", "size"] {
            let mut json: serde_json::Value = serde_json::from_slice(&original).unwrap();
            match change {
                "url" => {
                    json["assets"][0]["browser_download_url"] =
                        "https://evil.example/update.exe".into()
                }
                "digest" => json["assets"][0]["digest"] = "sha256:invalid".into(),
                "checksum" => json["assets"]
                    .as_array_mut()
                    .unwrap()
                    .pop()
                    .map(|_| ())
                    .unwrap(),
                _ => json["assets"][0]["size"] = (MAX_DOWNLOAD + 1).into(),
            }
            assert!(
                find_update(&serde_json::to_vec(&json).unwrap(), Version(0, 1, 2)).is_err(),
                "{change}"
            );
        }
    }
    #[test]
    fn supports_standalone_x64_and_historical_names_without_selecting_other_architectures() {
        for suffix in ["", "-x64", "-windows-x64", "-x86", "-arm64"] {
            let mut json: serde_json::Value = serde_json::from_slice(&release("v0.1.3")).unwrap();
            let name = format!("MagicLanguageSwitcher-v0.1.3{suffix}.exe");
            json["assets"][0]["name"] = name.clone().into();
            json["assets"][0]["browser_download_url"] =
                format!("https://github.com/{REPOSITORY}/releases/download/v0.1.3/{name}").into();
            let result = find_update(&serde_json::to_vec(&json).unwrap(), Version(0, 1, 2));
            if matches!(suffix, "-x86" | "-arm64") {
                assert!(result.is_err(), "{suffix}");
            } else {
                assert_eq!(result.unwrap().unwrap().filename, name);
            }
        }
    }

    #[test]
    fn prefers_the_standalone_asset_and_checks_its_exact_checksum_name() {
        let mut json: serde_json::Value = serde_json::from_slice(&release("v0.1.3")).unwrap();
        let mut legacy = json["assets"][0].clone();
        legacy["name"] = "MagicLanguageSwitcher-v0.1.3-windows-x64.exe".into();
        legacy["browser_download_url"] = format!("https://github.com/{REPOSITORY}/releases/download/v0.1.3/MagicLanguageSwitcher-v0.1.3-windows-x64.exe").into();
        json["assets"].as_array_mut().unwrap().insert(0, legacy);
        let update = find_update(&serde_json::to_vec(&json).unwrap(), Version(0, 1, 2))
            .unwrap()
            .unwrap();
        assert_eq!(update.filename, "MagicLanguageSwitcher-v0.1.3.exe");
        assert!(
            verify_checksum_file(
                format!("{}  {}\n", update.sha256, update.filename).as_bytes(),
                &update
            )
            .is_ok()
        );
        assert!(
            verify_checksum_file(
                format!(
                    "{}  MagicLanguageSwitcher-v0.1.3-windows-x64.exe\n",
                    update.sha256
                )
                .as_bytes(),
                &update
            )
            .is_err()
        );
    }
    #[test]
    fn validates_checksum_for_the_exact_asset() {
        let update = find_update(&release("v0.1.3"), Version(0, 1, 2))
            .unwrap()
            .unwrap();
        let line = format!("{}  {}\n", update.sha256, update.filename);
        assert!(verify_checksum_file(line.as_bytes(), &update).is_ok());
        assert!(verify_checksum_file(line.replace("ab", "cd").as_bytes(), &update).is_err());
        assert!(verify_checksum_file(format!("{line}{line}").as_bytes(), &update).is_err());
    }
}
