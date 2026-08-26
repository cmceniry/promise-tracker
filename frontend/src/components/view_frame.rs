use leptos::children::ViewFn;
use leptos::ev;
use leptos::prelude::*;

use crate::utils::download_html;

/// Chrome shared by the three contract views: a small toolbar, a full-page
/// maximize, and a download of the view as a standalone page.
///
/// The children are never unmounted — maximizing only adds a class to the root.
/// That matters for the graph: unmounting would tear down its Cytoscape
/// instance and restore a viewport measured for the old, smaller box.
#[component]
pub fn ViewFrame(
    /// Toolbar caption, and the title of the exported page
    #[prop(into)]
    title: Signal<String>,
    /// Second line of the exported page's header
    #[prop(into)]
    subtitle: Signal<String>,
    /// Name for the downloaded file, without the extension
    #[prop(into)]
    filename: Signal<String>,
    /// Builds the standalone document when the button is pressed. Returning
    /// None means there is nothing to export yet, and greys the button out.
    export: Callback<(), Option<String>>,
    /// Extra toolbar controls, placed before the frame's own buttons
    #[prop(optional, into)]
    toolbar: ViewFn,
    /// Fired after the frame expands (true) or collapses (false), for a view
    /// that has to re-measure a canvas
    #[prop(optional)]
    on_maximized: Option<Callback<bool>>,
    children: Children,
) -> impl IntoView {
    let (maximized, set_maximized) = signal(false);

    let set_state = move |next: bool| {
        set_maximized.set(next);
        if let Some(cb) = on_maximized {
            cb.run(next);
        }
    };

    // Escape leaves the maximized view, matching the contract modals - but not
    // while one of those modals is open, since it sits above us and owns the key.
    let handle = window_event_listener(ev::keydown, move |ev| {
        if ev.key() != "Escape" || !maximized.get_untracked() {
            return;
        }
        let modal_open = document()
            .query_selector(".modal.show")
            .ok()
            .flatten()
            .is_some();
        if modal_open {
            return;
        }
        ev.prevent_default();
        set_state(false);
    });
    on_cleanup(move || handle.remove());

    let on_download = move |_| {
        if let Some(html) = export.run(()) {
            download_html(&format!("{}.html", filename.get_untracked()), &html);
        }
    };

    view! {
        <div class="view-frame" class:view-maximized=move || maximized.get()>
            <div class="view-frame-toolbar">
                <div style="min-width: 0;">
                    <div class="view-frame-title">{move || title.get()}</div>
                    <Show when=move || !subtitle.get().is_empty()>
                        <div class="view-frame-subtitle">{move || subtitle.get()}</div>
                    </Show>
                </div>
                <div class="view-frame-actions">
                    {toolbar.run()}
                    <button
                        type="button"
                        class="btn btn-sm btn-outline-secondary"
                        aria-label="Download this view as a standalone page"
                        title="Download this view as a standalone HTML page"
                        on:click=on_download
                    >
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="16"
                            height="16"
                            fill="currentColor"
                            viewBox="0 0 16 16"
                        >
                            <path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"></path>
                            <path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z"></path>
                        </svg>
                    </button>
                    <button
                        type="button"
                        class="btn btn-sm btn-outline-secondary"
                        aria-label=move || {
                            if maximized.get() { "Back to the page" } else { "Fill the page" }
                        }
                        title=move || {
                            if maximized.get() {
                                "Back to the page (Esc)"
                            } else {
                                "Fill the page with this view"
                            }
                        }
                        on:click=move |_| set_state(!maximized.get_untracked())
                    >
                        {move || {
                            if maximized.get() {
                                // arrows pulling back to a corner
                                view! {
                                    <svg
                                        xmlns="http://www.w3.org/2000/svg"
                                        width="16"
                                        height="16"
                                        fill="currentColor"
                                        viewBox="0 0 16 16"
                                    >
                                        <path d="M5.5 0a.5.5 0 0 1 .5.5v4A1.5 1.5 0 0 1 4.5 6h-4a.5.5 0 0 1 0-1h4a.5.5 0 0 0 .5-.5v-4a.5.5 0 0 1 .5-.5m5 0a.5.5 0 0 1 .5.5v4a.5.5 0 0 0 .5.5h4a.5.5 0 0 1 0 1h-4A1.5 1.5 0 0 1 10 4.5v-4a.5.5 0 0 1 .5-.5M0 10.5a.5.5 0 0 1 .5-.5h4A1.5 1.5 0 0 1 6 11.5v4a.5.5 0 0 1-1 0v-4a.5.5 0 0 0-.5-.5h-4a.5.5 0 0 1-.5-.5m10 1a1.5 1.5 0 0 1 1.5-1.5h4a.5.5 0 0 1 0 1h-4a.5.5 0 0 0-.5.5v4a.5.5 0 0 1-1 0z"></path>
                                    </svg>
                                }
                                    .into_any()
                            } else {
                                // arrows pushing out to the corners
                                view! {
                                    <svg
                                        xmlns="http://www.w3.org/2000/svg"
                                        width="16"
                                        height="16"
                                        fill="currentColor"
                                        viewBox="0 0 16 16"
                                    >
                                        <path d="M1.5 1a.5.5 0 0 0-.5.5v4a.5.5 0 0 1-1 0v-4A1.5 1.5 0 0 1 1.5 0h4a.5.5 0 0 1 0 1zM10 .5a.5.5 0 0 1 .5-.5h4A1.5 1.5 0 0 1 16 1.5v4a.5.5 0 0 1-1 0v-4a.5.5 0 0 0-.5-.5h-4a.5.5 0 0 1-.5-.5M.5 10a.5.5 0 0 1 .5.5v4a.5.5 0 0 0 .5.5h4a.5.5 0 0 1 0 1h-4A1.5 1.5 0 0 1 0 14.5v-4a.5.5 0 0 1 .5-.5m15 0a.5.5 0 0 1 .5.5v4a1.5 1.5 0 0 1-1.5 1.5h-4a.5.5 0 0 1 0-1h4a.5.5 0 0 0 .5-.5v-4a.5.5 0 0 1 .5-.5"></path>
                                    </svg>
                                }
                                    .into_any()
                            }
                        }}
                    </button>
                </div>
            </div>
            <div class="view-frame-body">{children()}</div>
        </div>
    }
}
