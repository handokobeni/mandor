//! Menu tray dinamis: run yang menunggu dev (maks. 5), Open Mandor, Quit.

use serde::Deserialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const TRAY_ID: &str = "main";
/// Event ke UI saat run dipilih dari tray.
pub const OPEN_RUN_EVENT: &str = "tray://open-run";
const MAX_WAITING: usize = 5;
const RUN_PREFIX: &str = "run:";

/// Run yang menunggu jawaban dev, dikirim UI.
#[derive(Debug, Clone, Deserialize)]
pub struct WaitingRun {
    pub run_id: String,
    pub key: String,
    pub title: String,
}

/// Entri menu `(id, label)` untuk run yang menunggu, dan tooltip tray.
fn entries(waiting: &[WaitingRun]) -> (Vec<(String, String)>, String) {
    let items = waiting
        .iter()
        .take(MAX_WAITING)
        .map(|w| {
            let mut title: String = w.title.chars().take(48).collect();
            if w.title.chars().count() > 48 {
                title.push('…');
            }
            (
                format!("{RUN_PREFIX}{}", w.run_id),
                format!("{}  {title}", w.key),
            )
        })
        .collect();
    let tooltip = match waiting.len() {
        0 => "Mandor".to_owned(),
        1 => "Mandor · 1 waiting for you".to_owned(),
        n => format!("Mandor · {n} waiting for you"),
    };
    (items, tooltip)
}

fn menu<R: Runtime, M: Manager<R>>(
    app: &M,
    waiting: &[WaitingRun],
) -> tauri::Result<(Menu<R>, String)> {
    let (items, tooltip) = entries(waiting);
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "open",
        "Open Mandor",
        true,
        None::<&str>,
    )?)?;
    if !items.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&MenuItem::with_id(
            app,
            "hdr",
            "Waiting for you",
            false,
            None::<&str>,
        )?)?;
        for (id, label) in items {
            menu.append(&MenuItem::with_id(app, id, label, true, None::<&str>)?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok((menu, tooltip))
}

fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Membuat tray saat start.
pub fn setup<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<TrayIcon<R>> {
    let (menu, tooltip) = menu(app, &[])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(tooltip);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| {
        let id = event.id.as_ref();
        if id == "open" {
            show_main(app);
        } else if id == "quit" {
            app.exit(0);
        } else if let Some(run_id) = id.strip_prefix(RUN_PREFIX) {
            show_main(app);
            let _ = app.emit(OPEN_RUN_EVENT, run_id.to_owned());
        }
    })
    .build(app)
}

/// Memperbarui menu dan tooltip tray dari daftar run yang menunggu.
pub fn update<R: Runtime>(app: &AppHandle<R>, waiting: &[WaitingRun]) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let (menu, tooltip) = menu(app, waiting)?;
    tray.set_menu(Some(menu))?;
    tray.set_tooltip(Some(tooltip))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(i: usize, title: &str) -> WaitingRun {
        WaitingRun {
            run_id: format!("r{i}"),
            key: format!("PGS-{i}"),
            title: title.to_owned(),
        }
    }

    #[test]
    fn maksimal_lima_run_dengan_judul_dipotong_dan_tooltip_jumlah() {
        let list: Vec<_> = (0..7).map(|i| w(i, "Judul")).collect();
        let (items, tooltip) = entries(&list);
        assert_eq!(items.len(), 5);
        assert_eq!(items[0], ("run:r0".to_owned(), "PGS-0  Judul".to_owned()));
        assert_eq!(tooltip, "Mandor · 7 waiting for you");

        let (items, tooltip) = entries(&[w(1, &"x".repeat(60))]);
        assert!(items[0].1.ends_with('…') && items[0].1.chars().count() == "PGS-1  ".len() + 49);
        assert_eq!(tooltip, "Mandor · 1 waiting for you");
        assert_eq!(entries(&[]).1, "Mandor");
    }
}
