use tokio::net::tcp::OwnedReadHalf;
use std::cmp::min;
use http::{Request, Uri, Method, Version, Response, StatusCode};
use httparse::{EMPTY_HEADER, Status};
use std::str::FromStr;
use http::header::{HeaderName, HeaderValue, SERVER, DATE};
use tokio::io::AsyncReadExt;
use httpdate::fmt_http_date;
use std::time::SystemTime;

static SERVER_NAME: &str = "Cluster23";

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
    let header_map = response.headers();
    let mut response_bytes = Vec::new();

    let crfl = "\r\n";
    let colon = ": ";
    let http_version: &str = "1.1";

    let status_code_line = format!("HTTP/{} {}",http_version, response.status().as_str());
    let mut status_code_line_bytes = status_code_line.as_bytes().to_owned();

    response_bytes.append(&mut status_code_line_bytes);
    response_bytes.append(&mut crfl.as_bytes().to_owned());

    for header_name in header_map.keys() {
        let header_value = header_map.get(header_name).unwrap();
        let mut header_value_bytes = header_value.as_bytes().to_owned();
        let mut header_name_bytes = header_name.as_str().as_bytes().to_owned();

        response_bytes.append(&mut header_name_bytes);
        response_bytes.append(&mut colon.as_bytes().to_owned());
        response_bytes.append(&mut header_value_bytes);
        response_bytes.append(&mut crfl.as_bytes().to_owned());
    }
    crate::info!("Response generated: {}",std::str::from_utf8(&response_bytes).unwrap());
    response_bytes
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
