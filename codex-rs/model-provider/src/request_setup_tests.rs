use super::model_client;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use codex_login::default_client::ClientRedirectPolicy;
use pretty_assertions::assert_eq;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

fn keep_alive_server() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let url = format!(
        "http://{}/v1/responses",
        listener.local_addr().expect("addr")
    );
    let connections = Arc::new(AtomicUsize::new(0));
    let accepted = Arc::clone(&connections);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            accepted.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || serve(stream));
        }
    });
    (url, connections)
}

fn serve(stream: TcpStream) {
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut writer = stream;
    let mut line = String::new();
    loop {
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            if line == "\r\n" {
                break;
            }
        }
        if writer
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
            .is_err()
        {
            return;
        }
    }
}

async fn get(url: &str, redirect_policy: ClientRedirectPolicy) -> String {
    let factory = HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault);
    let client = model_client(&factory, url.to_string(), redirect_policy).expect("client");
    client
        .get(url)
        .send()
        .await
        .expect("send")
        .text()
        .await
        .expect("body")
}

#[tokio::test]
async fn model_requests_reuse_one_connection() {
    let (url, connections) = keep_alive_server();
    for _ in 0..3 {
        assert_eq!(get(&url, ClientRedirectPolicy::Default).await, "ok");
    }
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn changed_client_inputs_build_a_new_client() {
    let (url, connections) = keep_alive_server();
    assert_eq!(get(&url, ClientRedirectPolicy::Default).await, "ok");
    assert_eq!(get(&url, ClientRedirectPolicy::Reject).await, "ok");
    assert_eq!(connections.load(Ordering::SeqCst), 2);
}
