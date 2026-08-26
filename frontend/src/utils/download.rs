//! Handing a generated file to the browser.

use gloo_timers::callback::Timeout;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, BlobPropertyBag, HtmlElement, Url};

/// Save `html` as `filename` through the browser's download machinery.
///
/// Built on demand rather than through a reactive `Memo` of an object URL: an
/// export is expensive to generate and depends on the view's current state, and
/// minting one URL per state change would leak all but the last.
pub fn download_html(filename: &str, html: &str) {
    let Some(url) = object_url(html) else {
        web_sys::console::error_1(&"Failed to build the download".into());
        return;
    };

    if let Err(e) = click_download(filename, &url) {
        web_sys::console::error_1(&e);
    }

    // Revoking straight after the click can race the browser's own read of the
    // blob, so let it settle first.
    let to_revoke = url.clone();
    Timeout::new(1_000, move || {
        let _ = Url::revoke_object_url(&to_revoke);
    })
    .forget();
}

fn object_url(html: &str) -> Option<String> {
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(html));

    let options = BlobPropertyBag::new();
    options.set_type("text/html");

    Blob::new_with_str_sequence_and_options(&parts, &options)
        .ok()
        .and_then(|blob| Url::create_object_url_with_blob(&blob).ok())
}

/// Click a detached anchor. Detached keeps it out of the document, so this
/// needs neither the `Node` nor the `HtmlAnchorElement` web-sys feature.
fn click_download(filename: &str, url: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;

    let anchor = document.create_element("a")?;
    anchor.set_attribute("download", filename)?;
    anchor.set_attribute("href", url)?;
    anchor.dyn_into::<HtmlElement>()?.click();

    Ok(())
}
