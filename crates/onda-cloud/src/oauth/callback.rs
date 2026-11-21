use super::*;

pub(super) struct Callback {
    pub(super) code: Zeroizing<String>,
    pub(super) client_id: String,
}

pub(super) async fn receive_callback(pending: &PendingSignIn) -> Result<Callback> {
    for _ in 0..MAX_CALLBACK_CONNECTIONS {
        let (mut socket, address) = pending
            .listener
            .accept()
            .await
            .map_err(|_| CloudError::Transport)?;
        if !address.ip().is_loopback() {
            continue;
        }
        let request = match tokio::time::timeout(SOCKET_TIMEOUT, read_request(&mut socket)).await {
            Ok(Ok(request)) => request,
            _ => {
                write_callback_response(&mut socket, false).await;
                continue;
            }
        };
        let result = parse_callback(&request, pending);
        match result {
            Ok(Some(callback)) => {
                write_callback_response(&mut socket, true).await;
                return Ok(callback);
            }
            Ok(None) => {
                write_callback_response(&mut socket, false).await;
            }
            Err(error) => {
                write_callback_response(&mut socket, false).await;
                return Err(error);
            }
        }
    }
    Err(CloudError::InvalidCallback)
}

pub(super) async fn read_request(socket: &mut TcpStream) -> Result<Zeroizing<String>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(2048));
    let mut buffer = [0u8; 2048];
    loop {
        let read = socket
            .read(&mut buffer)
            .await
            .map_err(|_| CloudError::InvalidCallback)?;
        if read == 0 {
            return Err(CloudError::InvalidCallback);
        }
        if bytes.len() + read > MAX_HTTP_BYTES {
            return Err(CloudError::SizeLimit);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8(std::mem::take(&mut *bytes))
        .map(Zeroizing::new)
        .map_err(|_| CloudError::InvalidCallback)
}

pub(super) fn parse_callback(request: &str, pending: &PendingSignIn) -> Result<Option<Callback>> {
    let mut lines = request.split("\r\n");
    let mut request_line = lines
        .next()
        .ok_or(CloudError::InvalidCallback)?
        .split_whitespace();
    if request_line.next() != Some("GET") {
        return Err(CloudError::InvalidCallback);
    }
    let target = request_line.next().ok_or(CloudError::InvalidCallback)?;
    if request_line.next() != Some("HTTP/1.1")
        || request_line.next().is_some()
        || !target.starts_with('/')
        || target.starts_with("//")
    {
        return Err(CloudError::InvalidCallback);
    }
    let expected_authority = pending
        .redirect_uri
        .strip_prefix("http://")
        .and_then(|uri| uri.strip_suffix(CALLBACK_PATH))
        .ok_or(CloudError::Configuration)?;
    let hosts: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim())
        .collect();
    if hosts != [expected_authority] {
        return Err(CloudError::InvalidCallback);
    }
    let url = Url::parse(&format!("http://{expected_authority}{target}"))
        .map_err(|_| CloudError::InvalidCallback)?;
    if url.path() != CALLBACK_PATH {
        return Ok(None);
    }
    let mut values = HashMap::new();
    for (key, value) in url.query_pairs() {
        if values
            .insert(key.into_owned(), Zeroizing::new(value.into_owned()))
            .is_some()
        {
            return Err(CloudError::InvalidCallback);
        }
    }
    let state = values.get("state").ok_or(CloudError::InvalidCallback)?;
    if state.as_bytes().ct_eq(pending.state.as_bytes()).unwrap_u8() != 1 {
        return Err(CloudError::InvalidCallback);
    }
    if let Some(error) = values.get("error") {
        return Err(if error.as_str() == "access_denied" {
            CloudError::AccessDenied
        } else {
            CloudError::InvalidCallback
        });
    }
    let code = values
        .remove("code")
        .filter(|code| !code.is_empty() && code.len() <= 8192)
        .ok_or(CloudError::InvalidCallback)?;
    let issued = values.remove("client_id").map(|client| client.to_string());
    let client_id = if let Some(expected) = &pending.expected_client_id {
        if issued.as_ref().is_some_and(|issued| issued != expected) {
            return Err(CloudError::InvalidCallback);
        }
        expected.clone()
    } else {
        issued
            .filter(|client| valid_issued_client(client))
            .ok_or(CloudError::InvalidCallback)?
    };
    Ok(Some(Callback { code, client_id }))
}

pub(super) fn valid_issued_client(client: &str) -> bool {
    client != DYNAMIC_CLIENT
        && !client.is_empty()
        && client.len() <= 256
        && client
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub(super) async fn write_callback_response(socket: &mut TcpStream, success: bool) {
    let (status, body) = if success {
        (
            "200 OK",
            "Sign-in received. You may return to the application.",
        )
    } else {
        (
            "400 Bad Request",
            "Sign-in could not be verified. Return to the application.",
        )
    };
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    let _ = tokio::time::timeout(SOCKET_TIMEOUT, socket.write_all(response.as_bytes())).await;
}

pub(super) fn random_secret(length: usize) -> Result<String> {
    let mut bytes = Zeroizing::new(vec![0; length]);
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| CloudError::Configuration)?;
    Ok(URL_SAFE_NO_PAD.encode(&*bytes))
}
