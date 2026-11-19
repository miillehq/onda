//! Keep the two desktop webviews on the app's own pages. External web links use the OS browser.
use tauri::{webview::NewWindowResponse, Manager, Runtime, Url, WebviewWindowBuilder};

pub(super) fn restrict<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    configured_dev_url: Option<&Url>,
) -> WebviewWindowBuilder<'a, R, M> {
    let development = (cfg!(debug_assertions) && !cfg!(feature = "e2e") && tauri::is_dev())
        .then(|| configured_dev_url.cloned())
        .flatten();
    builder
        .on_navigation(move |url| local_document(url, development.as_ref()))
        .on_new_window(|url, _| {
            if external_link(&url) && webbrowser::open(url.as_str()).is_err() {
                tracing::warn!(
                    subsystem = "navigation",
                    "system browser could not open the link"
                );
            }
            NewWindowResponse::Deny
        })
}

fn local_document(url: &Url, dev: Option<&Url>) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match (url.scheme(), url.host_str(), url.port()) {
        ("tauri", Some("localhost"), None) | ("http" | "https", Some("tauri.localhost"), None) => {
            true
        }
        _ => dev.is_some_and(|origin| url.origin() == origin.origin()),
    }
}

fn external_link(url: &Url) -> bool {
    url.scheme() == "https"
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
        && !local_document(url, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_navigation_and_external_links_use_separate_destinations() {
        for page in [
            "tauri://localhost/index.html#settings",
            "https://tauri.localhost/pill.html",
            "http://tauri.localhost/?load=1",
        ] {
            let page = Url::parse(page).unwrap();
            assert!(local_document(&page, None));
            assert!(!external_link(&page));
        }
        let manual = Url::parse("https://example.org/help").unwrap();
        assert!(!local_document(&manual, None));
        assert!(external_link(&manual));
        let dev = Url::parse("http://127.0.0.1:1420").unwrap();
        assert!(!local_document(&dev, None));
        assert!(local_document(&dev, Some(&dev)));
        assert!(!local_document(
            &Url::parse("http://127.0.0.1:1421").unwrap(),
            Some(&dev)
        ));
    }
}
