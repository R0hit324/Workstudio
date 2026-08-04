use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::WebSocketStream;

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Browser app served to anyone hitting `http://<host>:<port>/`.
pub const INDEX_HTML: &str = include_str!("web/index.html");

/// Route one incoming TCP connection.
///
/// A plain HTTP GET (browser navigation) is answered with the web editor page
/// and the socket is closed (`None`). A WebSocket upgrade is completed by a
/// manual RFC 6455 handshake and returned ready for the sync protocol. The
/// returned `bool` is true for browser clients (an `Origin` header was sent),
/// which speak the simple whole-file `code`/`presence` dialect.
pub async fn accept(
    stream: TcpStream,
) -> Result<Option<(WebSocketStream<BufReader<TcpStream>>, bool)>, Box<dyn std::error::Error + Send + Sync>> {
    let mut rd = BufReader::new(stream);
    let mut head = Vec::with_capacity(4096);
    let mut tmp = [0u8; 2048];
    loop {
        if head.len() >= 16384 || head.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        match rd.read(&mut tmp).await {
            Ok(0) => break,
            Ok(n) => head.extend_from_slice(&tmp[..n]),
            Err(_) => return Ok(None),
        }
    }

    let mut headers = [httparse::EMPTY_HEADER; 24];
    let mut req = httparse::Request::new(&mut headers);
    let parsed = matches!(req.parse(&head), Ok(httparse::Status::Complete(_)));
    let is_ws = parsed
        && req.headers.iter().any(|h| {
            h.name.eq_ignore_ascii_case("upgrade")
                && h.value.eq_ignore_ascii_case(b"websocket")
        });

    if !is_ws {
        serve_html(&mut rd).await?;
        return Ok(None);
    }

    // Browsers always send an Origin; desktop TUI clients do not.
    let is_web = req
        .headers
        .iter()
        .any(|h| h.name.eq_ignore_ascii_case("origin"));

    let key = req
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("sec-websocket-key"))
        .map(|h| String::from_utf8_lossy(h.value).to_string())
        .unwrap_or_default();
    let accept = ws_accept(&key);
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    rd.get_mut().write_all(resp.as_bytes()).await?;
    rd.get_mut().flush().await?;
    let ws = WebSocketStream::from_raw_socket(rd, Role::Server, None).await;
    Ok(Some((ws, is_web)))
}

fn ws_accept(key: &str) -> String {
    use base64::Engine;
    let mut h = Sha1::new();
    h.update(key.as_bytes());
    h.update(WS_GUID.as_bytes());
    let digest = h.finalize();
    base64::engine::general_purpose::STANDARD.encode(digest)
}

async fn serve_html(rd: &mut BufReader<TcpStream>) -> std::io::Result<()> {
    let body = INDEX_HTML.as_bytes();
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    rd.get_mut().write_all(resp.as_bytes()).await?;
    rd.get_mut().write_all(body).await?;
    rd.get_mut().flush().await
}
