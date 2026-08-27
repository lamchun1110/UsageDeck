use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

pub const TIMEOUT_TEST_CLIENT_LIMIT: Duration = Duration::from_millis(100);
pub const TIMEOUT_TEST_RESPONSE_DELAY: Duration = Duration::from_secs(1);

pub fn serve_once(status: u16, headers: &[(&str, &str)], body: &str) -> String {
    serve_once_after(Duration::ZERO, status, headers, body)
}

pub fn serve_once_after(
    delay: Duration,
    status: u16,
    headers: &[(&str, &str)],
    body: &str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock HTTP listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    let headers = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let body = body.to_owned();

    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        thread::sleep(delay);
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            429 => "Too Many Requests",
            _ => "Test Response",
        };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{headers}\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
    });

    format!("http://{address}")
}

/// One step in a [`serve_sequence`] script.
pub enum Step {
    /// Answer the request with a full response.
    Respond {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    },
    /// Accept the connection and close it without answering, producing a
    /// transport-level failure.
    Close,
}

/// Serves a fixed sequence of steps, one per connection, and records the
/// request line of every connection it serves. Connections beyond the script
/// are refused (the listener is gone), which surfaces as a connection error
/// rather than a response.
pub fn serve_sequence(steps: Vec<Step>) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use std::sync::{Arc, Mutex};

    let listener = TcpListener::bind("127.0.0.1:0").expect("mock HTTP listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);

    thread::spawn(move || {
        for step in steps {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            let request_line = String::from_utf8_lossy(&request)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned();
            recorded.lock().unwrap().push(request_line);
            match step {
                Step::Close => continue,
                Step::Respond {
                    status,
                    headers,
                    body,
                } => {
                    let reason = match status {
                        200 => "OK",
                        429 => "Too Many Requests",
                        _ => "Test Response",
                    };
                    let headers = headers
                        .iter()
                        .map(|(name, value)| format!("{name}: {value}\r\n"))
                        .collect::<String>();
                    let response = format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{headers}\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        }
    });

    (format!("http://{address}"), requests)
}

/// Serves exactly one request, returns `status`/`body`, and hands the raw
/// captured request back through the channel so tests can assert on headers
/// the client sent.
pub fn capture_once(
    status: u16,
    body: &str,
) -> (
    String,
    std::sync::mpsc::Receiver<String>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock HTTP listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    let (sender, receiver) = std::sync::mpsc::channel();
    let body = body.to_owned();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        // Read a full request: headers plus any Content-Length body.
        let mut request = Vec::new();
        loop {
            let mut chunk = [0_u8; 1024];
            let count = stream.read(&mut chunk).unwrap_or(0);
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
            let text = String::from_utf8_lossy(&request);
            let Some(header_end) = text.find("\r\n\r\n") else {
                continue;
            };
            let content_length = text[..header_end]
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
        let _ = sender.send(String::from_utf8_lossy(&request).into_owned());
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
            body.len(),
            reason = if status == 200 { "OK" } else { "Test Response" },
        );
        let _ = stream.write_all(response.as_bytes());
    });
    (format!("http://{address}"), receiver, handle)
}
