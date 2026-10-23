//! 月の夜や 窓は閉じても 灯は消さず

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::audio::SourceKind;
use crate::commands::AppState;
use crate::state::Snapshot;

pub struct TrayItems {
    system: CheckMenuItem<Wry>,
    mic: CheckMenuItem<Wry>,
    shown: (bool, bool),
}

impl TrayItems {
    pub fn sync(&mut self, snapshot: &Snapshot) {
        let live = |kind: &str| snapshot.broadcasts.iter().any(|stream| stream.kind == kind);
        let state = (live("system"), live("microphone"));
        if state == self.shown {
            return;
        }
        let _ = self.system.set_checked(state.0);
        let _ = self.mic.set_checked(state.1);
        self.shown = state;
    }
}

pub fn show_main(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    let _ = app.emit("loco://shown", ());
}

pub fn hide_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

fn toggle(app: &AppHandle, kind: SourceKind) {
    let Some(service) = app
        .try_state::<AppState>()
        .and_then(|state| state.service.clone())
    else {
        return;
    };
    std::thread::spawn(move || {
        let live = service
            .snapshot()
            .broadcasts
            .iter()
            .any(|stream| stream.kind == kind.label());
        let result = if live {
            service.stop_stream(kind)
        } else {
            service.start_stream(kind)
        };
        if let Err(error) = result {
            log::warn!("tray toggle failed: {error}");
        }
    });
}

pub fn build(app: &AppHandle) -> tauri::Result<TrayItems> {
    let show = MenuItem::with_id(app, "show", "打开", true, None::<&str>)?;
    let system = CheckMenuItem::with_id(app, "system", "系统声音", true, false, None::<&str>)?;
    let mic = CheckMenuItem::with_id(app, "mic", "麦克风", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &system,
            &mic,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("locoSaonduh")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            "system" => toggle(app, SourceKind::System),
            "mic" => toggle(app, SourceKind::Microphone),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(TrayItems {
        system,
        mic,
        shown: (false, false),
    })
}
