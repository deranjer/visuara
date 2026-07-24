//! Serves the built React SPA (`web/dist`, embedded into the binary at
//! compile time) as static assets, falling back to `index.html` for
//! client-side routes so a hard refresh / direct URL entry works.
//!
//! `web/dist` must exist before `cargo build -p visuara-signaling` runs —
//! the `#[folder]` path below is resolved at compile time. Run
//! `npm --prefix web ci && npm --prefix web run build` once before your
//! first native build, or use the Vite dev-server workflow (`npm run dev`
//! in `web/`, proxying `/api/v1` and `/ws` to a locally running
//! `cargo run -p visuara-signaling`) while iterating on frontend-only
//! changes.

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "../web/dist/"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if let Some(file) = Assets::get(path) {
        return serve(file);
    }
    match Assets::get("index.html") {
        Some(file) => serve(file),
        None => {
            (StatusCode::NOT_FOUND, "frontend not built — run `npm --prefix web run build`").into_response()
        }
    }
}

fn serve(file: rust_embed::EmbeddedFile) -> Response {
    let mime = file.metadata.mimetype().to_string();
    ([(header::CONTENT_TYPE, mime)], file.data.into_owned()).into_response()
}
