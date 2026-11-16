//! Now playing via Windows GlobalSystemMediaTransportControls (SMTC).
use crate::state::Hub;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::sync::{OnceLock, mpsc::{Sender, channel}};
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

/// What the worker was told since it last ran.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Pending {
    session: bool,
    props: bool,
    playback: bool,
}

#[derive(Debug, Clone, Copy)]
enum Sig {
    Session,
    Props,
    Playback,
}

/// Bursts of signals collapse into one refresh.
fn coalesce(sigs: impl IntoIterator<Item = Sig>) -> Pending {
    let mut p = Pending::default();
    for s in sigs {
        match s {
            Sig::Session => p.session = true,
            Sig::Props => p.props = true,
            Sig::Playback => p.playback = true,
        }
    }
    p
}

/// A playback-only change reuses the cached title/artist/art.
fn needs_props_read(p: Pending, have_cache: bool) -> bool {
    p.session || p.props || !have_cache
}

/// Art is re-read only for a new track, or when the last read found none (thumbnail may arrive late).
fn needs_art_reload(cached: Option<&(String, String, bool)>, title: &str, artist: &str) -> bool {
    match cached {
        Some((t, a, has_art)) => t != title || a != artist || !has_art,
        None => true,
    }
}

type Token = Option<i64>;

#[derive(Default)]
struct Worker {
    session: Option<(Session, Token, Token)>,
    /// (title, artist, art) of the last read, so playback-only changes do not touch the thumbnail.
    cache: Option<(String, String, Option<String>)>,
    last_emitted: Option<Option<MediaState>>,
}

impl Worker {
    fn unsubscribe(&mut self) {
        if let Some((old, a, b)) = self.session.take() {
            if let Some(a) = a {
                let _ = old.RemoveMediaPropertiesChanged(a);
            }
            if let Some(b) = b {
                let _ = old.RemovePlaybackInfoChanged(b);
            }
        }
    }

    fn subscribe(&mut self, manager: &Manager_, tx: &Sender<Sig>) {
        self.unsubscribe();
        self.cache = None;
        if let Ok(s) = manager.GetCurrentSession() {
            let t = tx.clone();
            let a = s.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                let _ = t.send(Sig::Props);
                Ok(())
            }));
            let t = tx.clone();
            let b = s.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                let _ = t.send(Sig::Playback);
                Ok(())
            }));
            // Each token is kept on its own so a half-failed subscribe is still cleaned up.
            self.session = Some((s, a.ok(), b.ok()));
        }
    }

    fn read(&mut self, manager: &Manager_, p: Pending) -> Option<MediaState> {
        let session = manager.GetCurrentSession().ok()?;
        let playing = session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(is_playing).unwrap_or(false);
        if needs_props_read(p, self.cache.is_some()) {
            let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
            let title = props.Title().ok()?.to_string();
            let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
            let cached = self.cache.as_ref().map(|(t, a, art)| (t.clone(), a.clone(), art.is_some()));
            let art = if needs_art_reload(cached.as_ref(), &title, &artist) {
                props.Thumbnail().ok().and_then(|t| read_art(&t))
            } else {
                self.cache.as_ref().and_then(|c| c.2.clone())
            };
            self.cache = Some((title, artist, art));
        }
        let (title, artist, art) = self.cache.clone()?;
        build_state(title, artist, art, playing)
    }

    fn refresh(&mut self, manager: &Manager_, tx: &Sender<Sig>, app: &AppHandle, p: Pending) {
        if p.session {
            self.subscribe(manager, tx);
        }
        let state = self.read(manager, p);
        if self.last_emitted.as_ref() != Some(&state) {
            app.state::<Hub>().emit("media://changed", state.clone());
            self.last_emitted = Some(state);
        }
    }
}

static MANAGER: OnceLock<Manager_> = OnceLock::new();

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
        let _ = MANAGER.set(manager.clone());
        let (tx, rx) = channel::<Sig>();
        let t = tx.clone();
        // Handlers only signal; all blocking WinRT reads happen on this worker thread.
        let _token = manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t.send(Sig::Session);
            Ok(())
        }));
        let mut w = Worker::default();
        w.refresh(&manager, &tx, &app, Pending { session: true, ..Pending::default() });
        // Loop keeps the manager and session subscriptions alive for the app's lifetime.
        while let Ok(first) = rx.recv() {
            let p = coalesce(std::iter::once(first).chain(rx.try_iter()));
            w.refresh(&manager, &tx, &app, p);
        }
    });
}

/// Async so the WinRT call stays off the UI thread.
#[tauri::command]
pub async fn media_control(action: String) -> Result<(), String> {
    if !matches!(action.as_str(), "prev" | "toggle" | "next") {
        return Err(format!("unknown media action: {action}"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let manager = MANAGER.get().ok_or("media not ready")?;
        let session = manager.GetCurrentSession().map_err(|e| e.to_string())?;
        match action.as_str() {
            "prev" => session.TrySkipPreviousAsync(),
            "toggle" => session.TryTogglePlayPauseAsync(),
            _ => session.TrySkipNextAsync(),
        }
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
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
    fn signals_coalesce() {
        assert_eq!(coalesce([Sig::Props, Sig::Props, Sig::Playback]), Pending { session: false, props: true, playback: true });
        assert_eq!(coalesce([]), Pending::default());
    }

    #[test]
    fn playback_only_skips_props_read() {
        let pb = Pending { playback: true, ..Pending::default() };
        assert!(!needs_props_read(pb, true));
        assert!(needs_props_read(pb, false));
        assert!(needs_props_read(Pending { props: true, ..pb }, true));
    }

    #[test]
    fn art_reloads_only_on_new_track_or_missing_art() {
        let c = ("t".to_string(), "a".to_string(), true);
        assert!(!needs_art_reload(Some(&c), "t", "a"));
        assert!(needs_art_reload(Some(&c), "t2", "a"));
        assert!(needs_art_reload(Some(&("t".into(), "a".into(), false)), "t", "a"));
        assert!(needs_art_reload(None, "t", "a"));
    }

    #[test]
    fn unknown_action_is_err() {
        let r = tauri::async_runtime::block_on(media_control("bogus".into()));
        assert!(r.is_err());
    }
}
