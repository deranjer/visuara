//! Tiny shared HTML helpers for both the operator-only admin UI
//! (`admin.rs`) and the public user-facing web app (`web.rs`) — hand-written
//! `format!` strings, no template engine, appropriate at this project's
//! self-hosted scale.

use axum::http::{header, HeaderMap};

pub fn html_escape(input: &str) -> String {
    input.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>body{{font-family:sans-serif;max-width:640px;margin:2rem auto;padding:0 1rem}}\
         input{{display:block;width:100%;padding:.4rem;margin:.3rem 0 1rem}}\
         button{{padding:.5rem 1rem}}li{{margin:.4rem 0}}\
         nav a{{margin-right:1rem}}nav button{{margin-right:1rem}}\
         table{{border-collapse:collapse;width:100%}}\
         td,th{{text-align:left;padding:.3rem .6rem .3rem 0;border-bottom:1px solid #ddd}}\
         .dot{{display:inline-block;width:.6rem;height:.6rem;border-radius:50%;margin-right:.4rem}}\
         .online{{background:#2a2}}.offline{{background:#999}}</style></head>\
         <body><h1>{title}</h1>{body}</body></html>"
    )
}

/// Reads a single cookie's value by name out of the request headers.
pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}
