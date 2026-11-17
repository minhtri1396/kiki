use super::*;
use crate::testutil::serve;

#[test]
fn real_get_sends_basic_auth_and_returns_status_and_body() {
    let fake = serve(2, |_, url| {
        if url == "/missing" {
            (404, "nope".into())
        } else {
            (200, "{\"ok\":true}".into())
        }
    });
    let ctx = Ctx::background();
    let transport = UreqTransport;
    let url = format!("{}/ok?x=1", fake.base);
    let got = transport.send(&ctx, &HttpRequest::get(&url, "a@b.co", "tok")).unwrap();
    assert_eq!((got.status, got.body.as_slice()), (200, &b"{\"ok\":true}"[..]));
    let url = format!("{}/missing", fake.base);
    let got = transport.send(&ctx, &HttpRequest::get(&url, "a@b.co", "tok")).unwrap();
    assert_eq!((got.status, got.body.as_slice()), (404, &b"nope"[..]));
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen[0].0, "GET");
    assert_eq!(seen[0].1, "/ok?x=1");
    // base64("a@b.co:tok")
    assert_eq!(seen[0].2, "Basic YUBiLmNvOnRvaw==");
}

#[test]
fn real_post_and_delete_send_the_verb_and_payload() {
    let fake = serve(2, |method, _| match method {
        "POST" => (201, r#"{"id":"10","key":"kiki-1"}"#.into()),
        _ => (204, String::new()),
    });
    let ctx = Ctx::background();
    let url = format!("{}/rest/api/3/issue", fake.base);
    let got = UreqTransport
        .send(&ctx, &HttpRequest::post(&url, "a@b.co", "tok", b"{\"x\":1}"))
        .unwrap();
    assert_eq!(got.status, 201);
    assert_eq!(got.body, br#"{"id":"10","key":"kiki-1"}"#);
    let url = format!("{}/x", fake.base);
    let got = UreqTransport
        .send(&ctx, &HttpRequest::delete(&url, "a@b.co", "tok"))
        .unwrap();
    assert_eq!(got.status, 204);
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen[0].0, "POST");
    assert_eq!(seen[0].3, "{\"x\":1}");
    assert_eq!(seen[0].2, "Basic YUBiLmNvOnRvaw==");
    assert_eq!((seen[1].0.as_str(), seen[1].1.as_str()), ("DELETE", "/x"));
}

#[test]
fn real_calls_stop_when_the_context_is_done() {
    let ctx = Ctx::background().with_cancel();
    ctx.cancel();
    let request = HttpRequest::get("http://127.0.0.1:1/x", "u", "t");
    let err = UreqTransport.send(&ctx, &request).unwrap_err();
    assert!(err.to_string().contains("context canceled"), "err = {err}");
    let err = UreqTransport.send(&Ctx::background(), &request).unwrap_err();
    assert!(
        err.to_string().starts_with("Get \"http://127.0.0.1:1/x\": "),
        "err = {err}"
    );
}

#[test]
fn status_text_names_common_codes() {
    assert_eq!(status_text(404), "Not Found");
    assert_eq!(status_text(299), "");
}
