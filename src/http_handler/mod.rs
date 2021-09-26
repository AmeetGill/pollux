use tokio::net::tcp::OwnedReadHalf;
use std::cmp::min;
use http::{Request, Uri, Method, Version, Response, StatusCode, HeaderMap};
use httparse::{EMPTY_HEADER, Status};
use std::str::FromStr;
use http::header::{
    HeaderName,
    HeaderValue,
    SERVER,
    DATE,
    SEC_WEBSOCKET_KEY,
    SEC_WEBSOCKET_ACCEPT,
    UPGRADE,
    CONNECTION,
    SEC_WEBSOCKET_PROTOCOL,
    SEC_WEBSOCKET_VERSION,
    ORIGIN,
    HOST
};
use tokio::io::{AsyncReadExt};
use httpdate::fmt_http_date;
use std::time::SystemTime;
use sha1::{Sha1, Digest};
use crate::buffer::buffer::Buffer;
use std::error::Error;

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
static SUB_PROTOCOLS_SUPPORTED: [&str; 1] = ["v1.chat.cluster23.com"];
static PATHS_ALLOWED: [&str; 1] = ["/chat"];
static GET_METHOD: &str = "GET";
static WEBSOCKET_HEADERS_REQUIRED: [HeaderName;2] = [
    SEC_WEBSOCKET_KEY, // 16 byte
    // SEC_WEBSOCKET_PROTOCOL, if not present we will choose default
    SEC_WEBSOCKET_VERSION
];
static WEBSOCKET_VERSION_SUPPORTED: &str = "13";
lazy_static! {
    static ref HEADER_REQUIRED_MAP : [(HeaderName,String);4] = [
        // (ORIGIN,"cluster23.com"),
        (HOST,crate::MY_ADDRESS.to_ascii_lowercase()),
        (CONNECTION,"Upgrade".to_string()),
        (UPGRADE,"websocket".to_string()),
        (SEC_WEBSOCKET_VERSION,WEBSOCKET_VERSION_SUPPORTED.to_string())
    ];
}

static GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn get_http_response_bytes(response: Response<()>) -> Result<Vec<u8>,&'static str> {

    let mut buffer: Box<Buffer<u8>> = crate::buffer::buffer::Buffer::new_unbound();
    let header_map = response.headers();

    let crfl = "\r\n";
    let crfl_bytes = crfl.as_bytes();
    let colon = ": ";
    let colon_bytes = colon.as_bytes();
    let http_version: &str = "1.1";

    let status_code_line = format!("HTTP/{} {} {}",http_version, response.status().as_str(),response.status().canonical_reason().unwrap());
    let status_code_line_bytes = status_code_line.as_bytes();
    buffer.append_u8_array(&status_code_line_bytes)?;
    buffer.append_u8_array(crfl_bytes)?;

    for header_name in header_map.keys() {
        let header_value = header_map.get(header_name).unwrap();
        let header_value_bytes = header_value.as_bytes();
        let header_name_bytes = header_name.as_str().as_bytes();

        buffer.append_u8_array(header_name_bytes)?;
        buffer.append_u8_array(colon_bytes)?;
        buffer.append_u8_array(header_value_bytes)?;
        buffer.append_u8_array(crfl_bytes)?;
    }
    buffer.append_u8_array(crfl_bytes)?;
    let arr_final = buffer.get_arr();
    Ok(arr_final)
}

fn select_sub_protocol(header_map: &HeaderMap) -> Result<&str,&'static str> {
    // if header not present select the one available
    let protocols_header = match  header_map.get(SEC_WEBSOCKET_PROTOCOL) {
        None => { return Ok(SUB_PROTOCOLS_SUPPORTED[0])}
        Some(val) => val
    };
    let protocols_requested = protocols_header.to_str().unwrap().split(",");
    let mut protocol_matched: &str = "";
    for protocol_str in protocols_requested {
        if SUB_PROTOCOLS_SUPPORTED.contains(&protocol_str.trim()) {
            protocol_matched = protocol_str;
        }
    }
    if protocol_matched.len() > 0 {
        return Ok(protocol_matched);
    }
    return Err("Not supported protocol found");
}

fn header_value_matching(header_map: &HeaderMap, header_name: &HeaderName, header_value_expected: &str) -> bool {
    crate::info!("Checking \"{}\" Header Value",header_name.as_str());
    match header_map.get(header_name) {
        None => { return false}
        Some(header_value) => {
            if !header_value_expected.eq_ignore_ascii_case(header_value.to_str().unwrap()) {
                crate::error!("Expected value: {} for header: {} but found: {}",header_value_expected,header_name.as_str(),header_value.to_str().unwrap());
                return false;
            }
        }
    }
    true
}

pub fn can_be_upgraded_to_websocket(request: &Request<()> ) -> bool {
    crate::info!("Checking Websocket handshake Request");
    let mut path_matched = false;
    crate::info!("Checking Path/Resource");
    for path in PATHS_ALLOWED {
        if request.uri().to_string().eq_ignore_ascii_case(path) {
            path_matched = true;
            break;
        }
    }

    if !path_matched {
        crate::error!("Path requested: \"{}\" not found",request.uri().to_string());
        return false
    }

    crate::info!("Checking HTTP Method");
    if !request.method().as_str().eq_ignore_ascii_case(GET_METHOD) {
        crate::error!("HTTP method: \"{}\" not allowed",request.method().as_str());
        return false;
    }

    let header_map = request.headers();

    crate::info!("Checking Websocket Headers");
    for header_req in WEBSOCKET_HEADERS_REQUIRED.iter() {
        if !header_map.contains_key(header_req) {
            crate::error!("Header \"{}\" not found",header_req.as_str());
            return false;
        }
    }

    crate::info!("Checking Websocket Sub protocol");
    if request.headers().contains_key(SEC_WEBSOCKET_PROTOCOL) {
        match select_sub_protocol(header_map) {
            Err(_) => {
                crate::error!("Sub Protocol not supported ");
                return false
            }
            _ => {}
        }
    } else {
        crate::info!("No sub-protocol Requested");
    }

    crate::info!("Checking HTTP headers");
    for header_required in HEADER_REQUIRED_MAP.iter() {
        if !header_value_matching(
            header_map,
            &header_required.0,
            &header_required.1) {
            return false;
        }
    }

    let user_id_header: HeaderName = HeaderName::from_static("user-id");
    crate::info!("Checking User Id Header: {}",user_id_header);
    if !request.headers().contains_key(user_id_header) {
        crate::error!("No User-id Header found");
        return false;
    }


    true
}

pub fn create_websocket_response(request: Request<()>) -> Result<(Response<()>,u32),&'static str> {
    crate::info!("Creating Websocket handshake response");
    if !can_be_upgraded_to_websocket(&request) {
        crate::error!("Request not appropriate to make Handshake unsuccessful");
        return Err("Cannot be upgraded to websockets");
    }
    let mut response_builder = Response::builder();

    // The |Sec-WebSocket-Accept| header field indicates whether
    //    the server is willing to accept the connection
    let sec_ws_key = request.headers().get(SEC_WEBSOCKET_KEY).unwrap().to_str().unwrap();
    crate::info!("Creating Websocket Key response");

    if base64::decode(sec_ws_key).unwrap().len() != 16 {
        crate::error!("SEC_KEY length should be 16 bytes");
        return Err("SEC_KEY length should be 16 bytes");
    }

    let concat_key = format!("{}{}", sec_ws_key, GUID);
    let concat_key_u8 = concat_key.as_bytes();
    crate::info!("concat_key: {}",concat_key);

    let mut hasher = Sha1::new();
    hasher.update(concat_key_u8);
    let mut encoded_arr = hasher.finalize();
    crate::info!("Sha1 hash: {:?}",encoded_arr);

    let encoded_sha1 = base64::encode(encoded_arr);
    crate::info!("base64 hash: {}",encoded_sha1);


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
    crate::info!("Setting Required HTTP headers");
    response_builder = response_builder.status(StatusCode::SWITCHING_PROTOCOLS)
        .header(UPGRADE,HeaderValue::from_static("websocket"))
        .header(CONNECTION,HeaderValue::from_static("upgrade"))
        .header(SEC_WEBSOCKET_ACCEPT,HeaderValue::from_str(&encoded_sha1).unwrap())
        .header(SERVER,HeaderValue::from_static(SERVER_NAME))
        .header(DATE,fmt_http_date(SystemTime::now()));

    //if a sub prototcol is requested
    if request.headers().contains_key(SEC_WEBSOCKET_PROTOCOL) {
        response_builder = response_builder.header(SEC_WEBSOCKET_PROTOCOL,HeaderValue::from_str(select_sub_protocol(request.headers()).unwrap()).unwrap())
    }

    let user_id = request.headers().get(HeaderName::from_static("user-id"))
        .unwrap()
        .to_str().unwrap()
        .to_string()
        .parse::<u32>()
        .unwrap();

    // The server can also set cookie-related option fields to _set_
    //    cookies, as described in [RFC6265].
    Ok((response_builder.body(()).unwrap(),user_id))
}

pub fn create_401_response() -> Response<()>{
    Response::builder()
        .header(SEC_WEBSOCKET_VERSION,HeaderValue::from_static("13"))
        .status(StatusCode::BAD_REQUEST)
        .body(())
        .unwrap()
}


pub fn parse_http_request_bytes(bytes: Vec<u8>) -> Result<Request<()>,&'static str> {
    crate::info!("Parsing HTTP request");

    let mut headers = [EMPTY_HEADER;30];
    let mut http_parser_req = httparse::Request::new(&mut headers);
    match http_parser_req.parse(&bytes) {
        Ok(status) => {
            match status {
                Status::Complete(headers) => {
                    crate::info!("Parsing complete");
                }
                Status::Partial => {
                    crate::error!("Not able to completely parse request ");
                    return Err("Partial parsed request");
                }
            }
        }
        Err(e) => {
            crate::error!("Error while parsing http request: {} ",e);
            return Err("Error while parsing http request");
        }
    };


    if http_parser_req.version.is_none() {
        crate::error!("HTTP Version not found");
        return Err("Http Version not found");
    }

    if http_parser_req.method.is_none() {
        crate::error!("Http method not found");
        return Err("Http method not found");
    }

    if http_parser_req.path.is_none() {
        crate::error!("Http request Path not found");
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
                Err(e) => {
                    crate::error!("Error occurred: {}",e);
                    return Err("InvalidHeaderName")
                }
            };

            let header_value = match HeaderValue::from_bytes(header.value) {
                Ok(header_value) => header_value,
                Err(e) => {
                    crate::error!("Error occurred: {}",e);
                    return Err("InvalidHeaderValue")
                }
            };
            http_request_builder = http_request_builder.header(header_name, header_value);
        }
    }

    let http_request = http_request_builder.body(()).unwrap();
    if !http_request.method().as_str().trim().eq_ignore_ascii_case(GET_METHOD) {
        crate::error!("Http Method not support");
        return Err("Only HTTP GET method is allowed");
    }

    Ok(http_request)

}
