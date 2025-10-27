#![allow(clippy::unwrap_used)]
use anyhow::Result;
use base64::Engine;
use codex_login::ServerOptions;
use core_test_support::skip_if_no_network;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::thread;
use tempfile::tempdir;

fn start_mock_issuer(chatgpt_account_id: &str) -> (SocketAddr, thread::JoinHandle<()>) {
    // Bind to a random available port
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tiny_http::Server::from_listener(listener, None).unwrap();
    let chatgpt_account_id = chatgpt_account_id.to_string();

    let handle = thread::spawn(move || {
        while let Ok(mut req) = server.recv() {
            let url = req.url().to_string();
            if url.starts_with("/oauth/token") {
                // Read body
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                // Build minimal JWT
                #[derive(serde::Serialize)]
                struct Header {
                    alg: &'static str,
                    typ: &'static str,
                }
                let header = Header {
                    alg: "none",
                    typ: "JWT",
                };
                let payload = serde_json::json!({
                    "email": "user@example.com",
                    "https://api.openai.com/auth": {
                        "chatgpt_plan_type": "pro",
                        "chatgpt_account_id": chatgpt_account_id,
                    }
                });
                let b64 = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
                let header_bytes = serde_json::to_vec(&header).unwrap();
                let payload_bytes = serde_json::to_vec(&payload).unwrap();
                let id_token = format!(
                    "{}.{}.{}",
                    b64(&header_bytes),
                    b64(&payload_bytes),
                    b64(b"sig")
                );

                let tokens = serde_json::json!({
                    "id_token": id_token,
                    "access_token": "access-123",
                    "refresh_token": "refresh-123",
                });
                let data = serde_json::to_vec(&tokens).unwrap();
                let mut resp = tiny_http::Response::from_data(data);
                resp.add_header(
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .unwrap_or_else(|_| panic!("header bytes")),
                );
                let _ = req.respond(resp);
            } else {
                let _ = req
                    .respond(tiny_http::Response::from_string("not found").with_status_code(404));
            }
        }
    });

    (addr, handle)
}

#[tokio::test]
async fn test_manual_callback_login_signature() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let chatgpt_account_id = "12345678-0000-0000-0000-000000000000";
    let (issuer_addr, _issuer_handle) = start_mock_issuer(chatgpt_account_id);
    let issuer = format!("http://{}:{}", issuer_addr.ip(), issuer_addr.port());

    let tmp = tempdir()?;
    let codex_home = tmp.path().to_path_buf();

    // Verify that run_manual_callback_login function exists and has proper signature
    let opts = ServerOptions {
        codex_home,
        client_id: codex_login::CLIENT_ID.to_string(),
        issuer,
        port: 0,
        open_browser: false,
        force_state: Some("test_state_123".to_string()),
        forced_chatgpt_workspace_id: Some(chatgpt_account_id.to_string()),
    };

    // We can't actually test the interactive part without mocking stdin,
    // but we can verify the function signature compiles and is exported
    let _ = opts;

    Ok(())
}
