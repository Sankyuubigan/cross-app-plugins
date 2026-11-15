//! LegacyWebViewHost — обёртка над `gpui_webview::WebView` (фича `webview`).
//!
//! AIRSPACE RULE (from_Tauri_to_GPUI.md §2.3): на Windows нативный webview —
//! это дочернее HWND-окно, рисуемое **ПОВЕРХ** всей графики GPUI. Поэтому:
//! - перед открытием любой модалки/диалога/Select-списка, перекрывающего
//!   область webview: [`LegacyWebViewHost::hide`], после закрытия — `show`;
//! - при переключении на нативную вкладку GPUI — `hide` (состояние страницы
//!   сохраняется: hide прячет HWND, не выгружает страницу);
//! - выгрузку делать только при явном отключении legacy-режима.
//!
//! URL строится [`LegacyWebViewHost::local_url`]:
//! `http://127.0.0.1:<port>/#/<route>?token=<secret>` (hash-роут — токен не
//! попадает на сервер как query до `#`).

use gpui_kit::{App, AppContext, Entity, Window};
use gpui_webview::WebView;
use wry::WebViewBuilder;

/// Хост legacy-вебвью: entity-владелец `WebView` + построенный URL.
pub struct LegacyWebViewHost {
    pub webview: Entity<WebView>,
    pub url: String,
}

impl LegacyWebViewHost {
    /// Создать нативный webview-ребёнок окна (UI-поток: RootView::new/setup).
    pub fn build(url: impl Into<String>, window: &mut Window, cx: &mut App) -> wry::Result<Self> {
        let url = url.into();
        let builder = WebViewBuilder::new().with_url(url.clone());
        let webview = WebView::build(builder, window, cx)?;
        Ok(Self {
            webview: cx.new(|_| webview),
            url,
        })
    }

    /// URL локального legacy-фронта: `http://127.0.0.1:<port>/#/<route>?token=<secret>`.
    pub fn local_url(port: u16, route: &str, secret: &str) -> String {
        format!("http://127.0.0.1:{port}/#/{route}?token={secret}")
    }

    /// Спрятать HWND (модалки/переключение вкладки — Airspace Rule).
    pub fn hide<C: AppContext>(&mut self, cx: &mut C) {
        cx.update_entity(&self.webview, |w, _| w.hide());
    }

    /// Показать обратно (после закрытия модалки / возврата на вкладку).
    pub fn show<C: AppContext>(&mut self, cx: &mut C) {
        cx.update_entity(&self.webview, |w, _| w.show());
    }

    /// Виден ли webview сейчас.
    pub fn visible<C: AppContext>(&self, cx: &C) -> bool {
        cx.read_entity(&self.webview, |w, _| w.visible())
    }

    /// Нативная тема webview (тёмная/светлая рамка и скроллбары).
    pub fn set_native_theme<C: AppContext>(&self, dark: bool, cx: &C) {
        let theme = if dark { wry::Theme::Dark } else { wry::Theme::Light };
        cx.read_entity(&self.webview, |w, _| {
            if let Err(err) = w.set_theme(theme) {
                log::error!("webview: нативная тема не применена: {err:#}");
            }
        });
    }

    /// Инжект JS в страницу (синхронизация темы хоста с веб-стороны).
    pub fn eval<C: AppContext>(&self, js: &str, cx: &C) {
        cx.read_entity(&self.webview, |w, _| {
            if let Err(err) = w.raw().evaluate_script(js) {
                log::error!("webview: JS не выполнен: {err:#}");
            }
        });
    }

    /// Перейти на другой hash-роут того же origin (без пересоздания HWND).
    pub fn navigate<C: AppContext>(&mut self, url: &str, cx: &mut C) {
        self.url = url.to_string();
        cx.update_entity(&self.webview, |w, _| w.load_url(url));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// local_url: hash-роут + токен в query после `#`.
    #[test]
    fn local_url_contains_port_route_and_token() {
        let url = LegacyWebViewHost::local_url(5173, "draft", "sec123");
        assert_eq!(url, "http://127.0.0.1:5173/#/draft?token=sec123");
    }
}
