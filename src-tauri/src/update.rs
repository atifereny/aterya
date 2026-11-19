//! Self-update from GitHub Releases: check the latest release, download its exe, verify the size and SHA-256 that
//! GitHub reports, swap the running exe for the new one and restart. Release builds only.
//! The repository's releases must be readable without a login (a public repo) for this to work.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

const REPO: &str = "atifereny/aterya";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub size: u64,
    /// A check has finished (successfully or not).
    pub checked: bool,
    pub error: String,
    /// Dev builds can check but not install.
    pub can_install: bool,
}

#[derive(Clone, Debug)]
struct Asset {
    url: String,
    size: u64,
    sha256: Option<String>,
}

static STATE: Mutex<Option<(UpdateInfo, Option<Asset>)>> = Mutex::new(None);
static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// "v0.2.1" or "0.2.1-beta" to (0, 2, 1); missing parts are 0.
fn parse_version(s: &str) -> (u64, u64, u64) {
    let mut it = s.trim().trim_start_matches('v').split(['.', '-', '+']).map(|p| p.parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

fn base_info() -> UpdateInfo {
    UpdateInfo { current: CURRENT.into(), can_install: !cfg!(debug_assertions), ..Default::default() }
}

fn fetch_latest() -> Result<(UpdateInfo, Option<Asset>), String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Aterya-updater")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let r = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|_| "Güncelleme sunucusuna ulaşılamadı. İnternet bağlantını kontrol et.".to_string())?;
    if r.status().as_u16() == 404 {
        return Err("Yayınlanmış bir sürüm bulunamadı (depo gizli olabilir).".into());
    }
    if !r.status().is_success() {
        return Err(format!("Güncelleme sunucusu hata verdi ({}).", r.status().as_u16()));
    }
    let v: serde_json::Value = r.json().map_err(|_| "Sunucu yanıtı okunamadı.".to_string())?;
    let tag = v["tag_name"].as_str().unwrap_or("");
    let mut info = base_info();
    info.latest = tag.trim_start_matches('v').to_string();
    info.notes = v["body"].as_str().unwrap_or("").chars().take(4000).collect();
    info.checked = true;
    let asset = v["assets"].as_array().and_then(|a| {
        a.iter().find(|x| x["name"].as_str().is_some_and(|n| n.to_lowercase().ends_with(".exe"))).map(|x| Asset {
            url: x["browser_download_url"].as_str().unwrap_or("").to_string(),
            size: x["size"].as_u64().unwrap_or(0),
            sha256: x["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(|s| s.to_lowercase()),
        })
    });
    info.available = parse_version(tag) > parse_version(CURRENT) && asset.is_some();
    info.size = asset.as_ref().map_or(0, |a| a.size);
    Ok((info, asset))
}

fn check_now(app: &AppHandle) -> UpdateInfo {
    let (info, asset) = match fetch_latest() {
        Ok(x) => x,
        Err(e) => (UpdateInfo { checked: true, error: e, ..base_info() }, None),
    };
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some((info.clone(), asset));
    let _ = app.emit("update://state", info.clone());
    info
}

#[tauri::command]
pub fn update_state() -> UpdateInfo {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or_else(base_info, |(i, _)| i.clone())
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> UpdateInfo {
    tauri::async_runtime::spawn_blocking(move || check_now(&app)).await.unwrap_or_else(|_| base_info())
}

#[derive(Serialize, Clone)]
struct Progress {
    done: u64,
    total: u64,
}

fn install(app: &AppHandle) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("Geliştirme sürümü güncellenmez. Release sürümünde çalışır.".into());
    }
    let asset = STATE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|(_, a)| a.clone()).ok_or("Önce güncellemeleri denetle.")?;
    if !asset.url.starts_with(&format!("https://github.com/{REPO}/releases/download/")) {
        return Err("İndirme adresi beklenen depodan değil, güncelleme iptal edildi.".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("Uygulama klasörü bulunamadı.")?;
    let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("app");
    let (tmp, old) = (dir.join(format!("{stem}.update.tmp")), dir.join(format!("{stem}.old.exe")));

    let client = reqwest::blocking::Client::builder().user_agent("Aterya-updater").timeout(std::time::Duration::from_secs(600)).build().map_err(|e| e.to_string())?;
    let mut r = client.get(&asset.url).send().map_err(|_| "İndirme başlatılamadı.".to_string())?;
    if !r.status().is_success() {
        return Err(format!("İndirme hata verdi ({}).", r.status().as_u16()));
    }
    let total = r.content_length().unwrap_or(asset.size);
    let mut file = std::fs::File::create(&tmp).map_err(|e| format!("Geçici dosya yazılamadı: {e}"))?;
    let (mut hasher, mut done, mut buf, mut last) = (Sha256::new(), 0u64, vec![0u8; 64 * 1024], 0u64);
    let result = (|| -> Result<(), String> {
        loop {
            let n = r.read(&mut buf).map_err(|_| "İndirme yarıda kesildi.".to_string())?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            hasher.update(&buf[..n]);
            done += n as u64;
            if done - last > 256 * 1024 || done == total {
                last = done;
                let _ = app.emit("update://progress", Progress { done, total });
            }
        }
        file.flush().map_err(|e| e.to_string())?;
        if asset.size != 0 && done != asset.size {
            return Err("İndirilen dosyanın boyutu yanlış, güncelleme iptal edildi.".into());
        }
        if let Some(want) = &asset.sha256 {
            let got: String = hasher.clone().finalize().iter().map(|b| format!("{b:02x}")).collect();
            if &got != want {
                return Err("İndirilen dosya doğrulanamadı (SHA-256 uyuşmuyor), güncelleme iptal edildi.".into());
            }
        }
        Ok(())
    })();
    drop(file);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if std::fs::read(&tmp).ok().is_none_or(|b| b.len() < 2 || &b[..2] != b"MZ") {
        let _ = std::fs::remove_file(&tmp);
        return Err("İndirilen dosya bir Windows programı değil.".into());
    }
    // A running exe can be renamed but not overwritten: move it aside, put the new one in its place.
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| format!("Mevcut uygulama yerinden alınamadı: {e}"))?;
    if let Err(e) = std::fs::rename(&tmp, &exe) {
        let _ = std::fs::rename(&old, &exe); // put the old one back
        return Err(format!("Yeni sürüm yerleştirilemedi: {e}"));
    }
    std::process::Command::new(&exe).arg("--after-update").spawn().map_err(|e| format!("Yeni sürüm başlatılamadı: {e}"))?;
    Ok(())
}

/// Download, install and restart into the new version. On success the app exits.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    if BUSY.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Güncelleme zaten sürüyor.".into());
    }
    let a = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || install(&a)).await.map_err(|e| e.to_string());
    BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
    r??;
    log::info!("[update] new version started, exiting");
    app.exit(0); // teardown gives the Windows taskbar back; the new process hides it again
    Ok(())
}

/// Leftovers of a finished update: the old exe renamed aside, an interrupted download.
pub fn cleanup() {
    let Ok(exe) = std::env::current_exe() else { return };
    let (Some(dir), Some(stem)) = (exe.parent(), exe.file_stem().and_then(|s| s.to_str())) else { return };
    for name in [format!("{stem}.old.exe"), format!("{stem}.update.tmp")] {
        let p = dir.join(name);
        for _ in 0..5 {
            if !p.exists() || std::fs::remove_file(&p).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500)); // the old process may still be closing
        }
    }
}

/// Background check at startup and then every 12 hours, if the user allows it; installs by itself when asked to.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(25));
        loop {
            let cfg = crate::config::Config::load(&crate::config::config_path());
            if cfg.auto_update_check.unwrap_or(true) {
                let info = check_now(&app);
                if info.available && cfg.auto_update_install && !cfg!(debug_assertions) {
                    log::info!("[update] installing {} automatically", info.latest);
                    let a = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = update_install(a).await;
                    });
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(12 * 3600));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn versions_compare() {
        assert_eq!(parse_version("v0.2.1"), (0, 2, 1));
        assert_eq!(parse_version("1.0"), (1, 0, 0));
        assert_eq!(parse_version("0.1.0-beta.1"), (0, 1, 0));
        assert!(parse_version("v0.1.10") > parse_version("0.1.9"));
        assert!(parse_version("v1.0.0") > parse_version("0.9.9"));
        assert!(!(parse_version("v0.1.0") > parse_version("0.1.0")));
    }
}
