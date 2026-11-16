//! Now playing via Windows GlobalSystemMediaTransportControls (SMTC).
use crate::state::Hub;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use windows::{
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager_,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Storage::Streams::{DataReader, IRandomAccessStreamReference},
    Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx},
};

const MAX_ART_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaState {
    pub title: String,
    pub artist: String,
    pub art_data_url: Option<String>,
    pub playing: bool,
}

fn is_playing(s: Status) -> bool {
    s == Status::Playing
}

/// A session without a title is treated as no session.
fn build_state(title: String, artist: String, art_data_url: Option<String>, playing: bool) -> Option<MediaState> {
    if title.trim().is_empty() {
        return None;
    }
    Some(MediaState { title, artist, art_data_url, playing })
}

/// Oversized or empty images yield no art.
fn to_data_url(mime: &str, bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_ART_BYTES {
        return None;
    }
    let mime = if mime.is_empty() { "image/png" } else { mime };
    Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

fn read_art(r: &IRandomAccessStreamReference) -> Option<String> {
    let stream = r.OpenReadAsync().ok()?.join().ok()?;
    let size = usize::try_from(stream.Size().ok()?).ok()?;
    if size == 0 || size > MAX_ART_BYTES {
        return None;
    }
    let mime = stream.ContentType().map(|s| s.to_string()).unwrap_or_default();
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size as u32).ok()?.join().ok()?;
    let mut buf = vec![0u8; size];
    reader.ReadBytes(&mut buf).ok()?;
    to_data_url(&mime, &buf)
}

fn read_state(session: &Session) -> Option<MediaState> {
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let playing = session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(is_playing).unwrap_or(false);
    let art = props.Thumbnail().ok().and_then(|t| read_art(&t));
    build_state(props.Title().ok()?.to_string(), props.Artist().map(|s| s.to_string()).unwrap_or_default(), art, playing)
}

#[derive(Default)]
struct Subs {
    session: Option<(Session, i64, i64)>,
}

struct Inner {
    app: AppHandle,
    manager: Manager_,
    subs: Mutex<Subs>,
}

impl Inner {
    fn emit(&self) {
        let state = self.manager.GetCurrentSession().ok().and_then(|s| read_state(&s));
        self.app.state::<Hub>().emit("media://changed", state);
    }

    /// Moves the per-session subscriptions to the current session, dropping the old tokens.
    fn resubscribe(self: &Arc<Self>) {
        // Held across emit so concurrent handler threads cannot interleave and publish stale state.
        let mut subs = self.subs.lock().unwrap();
        if let Some((old, a, b)) = subs.session.take() {
            let _ = old.RemoveMediaPropertiesChanged(a);
            let _ = old.RemovePlaybackInfoChanged(b);
        }
        if let Ok(s) = self.manager.GetCurrentSession() {
            let me = self.clone();
            let props = s.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                me.emit_locked();
                Ok(())
            }));
            let me = self.clone();
            let playback = s.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                me.emit_locked();
                Ok(())
            }));
            if let (Ok(a), Ok(b)) = (props, playback) {
                subs.session = Some((s, a, b));
            }
        }
        self.emit();
    }

    fn emit_locked(&self) {
        let _g = self.subs.lock().unwrap();
        self.emit();
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let manager = match Manager_::RequestAsync().and_then(|o| o.join()) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("media manager unavailable: {e}");
                return;
            }
        };
        let inner = Arc::new(Inner { app, manager, subs: Mutex::default() });
        let me = inner.clone();
        let _token = inner.manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            me.resubscribe();
            Ok(())
        }));
        inner.resubscribe();
        // Keep `inner` (manager + subscriptions) alive for the app's lifetime.
        loop {
            std::thread::park();
        }
    });
}

#[tauri::command]
pub fn media_control(action: String) -> Result<(), String> {
    if !matches!(action.as_str(), "prev" | "toggle" | "next") {
        return Err(format!("unknown media action: {action}"));
    }
    let manager = Manager_::RequestAsync().and_then(|o| o.join()).map_err(|e| e.to_string())?;
    let session = manager.GetCurrentSession().map_err(|e| e.to_string())?;
    match action.as_str() {
        "prev" => session.TrySkipPreviousAsync(),
        "toggle" => session.TryTogglePlayPauseAsync(),
        _ => session.TrySkipNextAsync(),
    }
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_playing_status_is_playing() {
        assert!(is_playing(Status::Playing));
        assert!(!is_playing(Status::Paused));
        assert!(!is_playing(Status::Stopped));
    }

    #[test]
    fn empty_title_means_no_session() {
        assert_eq!(build_state("  ".into(), "a".into(), None, true), None);
        let s = build_state("t".into(), "a".into(), None, false).unwrap();
        assert_eq!((s.title.as_str(), s.playing), ("t", false));
    }

    #[test]
    fn data_url_defaults_mime_and_caps_size() {
        assert_eq!(to_data_url("", &[1, 2, 3]).unwrap(), "data:image/png;base64,AQID");
        assert!(to_data_url("image/jpeg", &[1]).unwrap().starts_with("data:image/jpeg;"));
        assert_eq!(to_data_url("image/png", &vec![0; MAX_ART_BYTES + 1]), None);
        assert_eq!(to_data_url("image/png", &[]), None);
    }

    #[test]
    fn unknown_action_is_err() {
        let r = media_control("bogus".into());
        assert!(r.is_err());
    }
}
