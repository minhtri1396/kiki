//! The web UI files (`../src`), embedded for the HTTP adapter. The Tauri window gets the same files
//! from Tauri's own asset protocol, so this is only used by `--no-open` (browser) mode.

use include_dir::{include_dir, Dir};
use kiki_server::http::{Request, Response};

static FRONTEND: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../src");

/// `GET /` and `GET /<file>` for a file of the frontend; `None` for everything else.
pub fn serve(request: &Request) -> Option<Response> {
    if request.method != "GET" && request.method != "HEAD" {
        return None;
    }
    let name = match request.path.as_str() {
        "/" => "index.html",
        path => path.trim_start_matches('/'),
    };
    let file = FRONTEND.get_file(name)?;
    Some(
        Response::new(200)
            .with_header("Content-Type", content_type(name))
            .with_header("Cache-Control", "no-store")
            .with_body(file.contents()),
    )
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_the_page_and_scripts() {
        let page = serve(&Request::new("GET", "/")).unwrap();
        assert_eq!(page.header("Content-Type"), "text/html; charset=utf-8");
        let script = serve(&Request::new("GET", "/transport.js")).unwrap();
        assert_eq!(script.header("Content-Type"), "text/javascript; charset=utf-8");
        assert!(script.into_text().contains("api_request"));
    }

    #[test]
    fn leaves_api_and_unknown_paths_alone() {
        assert!(serve(&Request::new("GET", "/api/state")).is_none());
        assert!(serve(&Request::new("POST", "/")).is_none());
        assert!(serve(&Request::new("GET", "/missing.js")).is_none());
    }
}
