use tokio::net::tcp::OwnedReadHalf;
use std::cmp::min;
use http::{Request, Uri, Method, Version, Response, StatusCode, HeaderMap};
use httparse::{EMPTY_HEADER, Status};
use std::str::FromStr;
use http::header::{HeaderName, HeaderValue, SERVER, DATE, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_ACCEPT, UPGRADE, CONNECTION, SEC_WEBSOCKET_PROTOCOL};
use tokio::io::AsyncReadExt;
use httpdate::fmt_http_date;
use std::time::SystemTime;
use crypto::digest::Digest;
use crate::buffer::buffer::Buffer;

static SERVER_NAME: &str = "Cluster23";


//  The client can request that the server use a specific subprotocol by
//    including the |Sec-WebSocket-Protocol| field in its handshake.  If it
//    is specified, the server needs to include the same field and one of
//    the selected subprotocol values in its response for the connection to
//    be established.
//
// These subprotocol names should be registered as per Section 11.5.  To
// avoid potential collisions, it is recommended to use names that
// contain the ASCII version of the domain name of the subprotocol's
// originator
static SUB_PROTOCOL_SUPPORTED: [&str; 1] = ["v1.chat.cluster23.com"];

static GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn get_websocket_http_response(status_code: StatusCode) -> Response<()> {
    Response::builder()
        .version(Version::HTTP_11)
        .status(status_code)
        .header(SERVER,HeaderValue::from_static(SERVER_NAME))
        .header(DATE,fmt_http_date(SystemTime::now()))
        .body(())
        .unwrap()
}

pub fn get_http_response_bytes(response: &mut Response<()>) -> Vec<u8> {

    let mut buffer: Box<Buffer<u8>> = crate::buffer::buffer::Buffer::new_unbound();
    let header_map = response.headers();

    let crfl = "\r\n";
    let crfl_bytes = crfl.as_bytes();
    let colon = ": ";
    let colon_bytes = colon.as_bytes();
    let http_version: &str = "1.1";

    let status_code_line = format!("HTTP/{} {}",http_version, response.status().as_str());
    let mut status_code_line_bytes = status_code_line.as_bytes();
    buffer.append_u8_array(&status_code_line_bytes);
    buffer.append_u8_array(crfl_bytes);

    for header_name in header_map.keys() {
        let header_value = header_map.get(header_name).unwrap();
        let mut header_value_bytes = header_value.as_bytes();
        let mut header_name_bytes = header_name.as_str().as_bytes();

        buffer.append_u8_array(header_name_bytes);
        buffer.append_u8_array(crfl_bytes);
        buffer.append_u8_array(header_value_bytes);
        buffer.append_u8_array(crfl_bytes);
    }
    let arr_final = buffer.get_arr();
    crate::info!("Response generated: {}",std::str::from_utf8(&arr_final).unwrap());
    arr_final
}

pub fn check_websocket_headers<'a>(request: &'a Request<()> , ws_protocol_selected: &'a str) -> Result<HeaderMap,&'static str> {
    let mut header_map: HeaderMap = HeaderMap::new();
    let mut response_builder = Response::builder();

    // The |Sec-WebSocket-Accept| header field indicates whether
    //    the server is willing to accept the connection
    let sec_ws_key = match request.headers().get(SEC_WEBSOCKET_KEY) {
        None => return Err("Sec-WebSocket-Key not found"),
        Some(sec_ws_key) => sec_ws_key.to_str().unwrap().trim()
    };

    let concat_key = format!("{}{}", sec_ws_key, GUID);
    crate::info!("concat_key: {}",concat_key);

    let mut sha1_hasher = crypto::sha1::Sha1::new();
    sha1_hasher.input_str(sec_ws_key);
    let mut encoded_arr = [1;20];
    sha1_hasher.result(&mut encoded_arr);
    crate::info!("Sha1 hash: {:?}",encoded_arr);
    crate::info!("base64 hash: {}",base64::encode(encoded_arr));

    let encoded_sha1 = base64::encode(encoded_arr);

    // any status code other than 101 indicates that the WebSocket handshake
    //    has not completed and that the semantics of HTTP still apply
    //
    //  HTTP/1.1 101 Switching Protocols
    //         Upgrade: websocket
    //         Connection: Upgrade
    //         Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=
    //         Sec-WebSocket-Protocol: chat
    //
    //    These fields are checked by the WebSocket client for scripted pages.
    //    If the |Sec-WebSocket-Accept| value does not match the expected
    //    value, if the header field is missing, or if the HTTP status code is
    //    not 101, the connection will not be established, and WebSocket frames
    //    will not be sent.
    response_builder.status("HTTP/1.1 101 Switching Protocols");
    header_map.append(UPGRADE,HeaderValue::from_static("websocket"));
    header_map.append(CONNECTION,HeaderValue::from_static("Upgrade"));
    header_map.append(SEC_WEBSOCKET_ACCEPT,HeaderValue::from_str(&*base64::encode(encoded_sha1.as_bytes())).unwrap());
    header_map.append(SEC_WEBSOCKET_PROTOCOL,HeaderValue::from_str(ws_protocol_selected).unwrap());

    // The server can also set cookie-related option fields to _set_
    //    cookies, as described in [RFC6265].
    Ok(header_map)
}

pub async fn read_bytes_from_socket(read_half: &mut OwnedReadHalf) -> Result<Vec<u8>, std::io::Error> {
    let mut bytes_data = vec![0;1024];
    let mut ptr: usize = 0;
    let mut data_size = 0;
    loop {
        crate::info!("Start reading Data from socket");
        let mut buf = [0; 256];
        let n: usize = match read_half.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => return Err(e)
        };
        if n <= 0 {
            break;
        }
        crate::info!("Read {} bytes",n);
        data_size += n;
        let end_ptr = min(n, bytes_data.len()-data_size);
        for i in 0..end_ptr {
            bytes_data[ptr] = buf[i];
            ptr += 1;
        }
        if n < buf.len() {
            break;
        }
        crate::info!("Copied Data");
    }
    crate::info!("Data read Complete");
    Ok(bytes_data[..data_size].to_owned())
}

pub fn parse_http_request_bytes(bytes: &[u8]) -> Result<Request<()>,&'static str> {

    let mut headers = [EMPTY_HEADER;10];
    let mut http_parser_req = httparse::Request::new(&mut headers);
    match http_parser_req.parse(bytes) {
        Ok(status) => {
            match status {
                Status::Complete(headers) => {
                    crate::info!("Number of headers: {:?}",headers);
                }
                Status::Partial => {}
            }
        }
        Err(_) => {}
    };


    if http_parser_req.version.is_none() {
        return Err("Http Version not found");
    }

    if http_parser_req.method.is_none() {
        return Err("Http method not found");
    }

    if http_parser_req.path.is_none() {
        return Err("Http request Path not found");
    }

    let mut http_request_builder = Request::builder()
        .uri(
            Uri::from_str(http_parser_req.path.unwrap())
                .unwrap()
        )
        .method(
            &Method::from_bytes(http_parser_req
                .method.unwrap()
                .to_ascii_lowercase()
                .as_bytes()).unwrap()
        )
        .version(Version::HTTP_11);

    for header in headers {
        if !header.name.is_empty() {
            let header_name = match HeaderName::from_bytes(header.name.to_ascii_lowercase().as_bytes()) {
                Ok(header_name) => header_name,
                Err(_e) => return Err("InvalidHeaderName")
            };

            let header_value = match HeaderValue::from_bytes(header.value) {
                Ok(header_value) => header_value,
                Err(_e) => return Err("InvalidHeaderValue")
            };
            http_request_builder = http_request_builder.header(header_name, header_value);
        }
    }

    let http_request = http_request_builder.body(()).unwrap();
    let get_method = "get";
    if !http_request.method().as_str().trim().eq_ignore_ascii_case(get_method) {
        return Err("Only HTTP GET method is allowed");
    }

    Ok(http_request)

}
