use tokio::net::{TcpListener, TcpStream};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::fmt::Debug;
use serde::{Serialize,Deserialize};
use httparse::{EMPTY_HEADER, Status};
use tokio::net::tcp::{OwnedReadHalf};
use std::cmp::min;
use hyper::{Request, Method, Version, Uri};
use hyper::http::header::HeaderName;
use hyper::http::HeaderValue;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::{Config};
use log::{info, LevelFilter};
use log4rs;
use std::str::FromStr;
use log4rs::encode::pattern::{PatternEncoder};

#[derive(Serialize, Deserialize, Debug)]
struct Json {
    company: String
}

#[tokio::main]
async fn main() {

    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {h({l})} {T} [{f:1.10}:{L}] [{M}] {m}{n}")))
        .build();

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(LevelFilter::Trace))
        .unwrap();

    let _handle = log4rs::init_config(config).unwrap();

    // Bind the listener to the address
    let listener = TcpListener::bind("127.0.0.1:1234").await.unwrap();

    loop {
        // The second item contains the IP and port of the new connection.
        info!("Waiting for Clients");
        let (socket, socket_address) = listener.accept().await.unwrap();
        info!("Client connected : {:?}", socket_address);
        tokio::spawn(async move{
            process(socket, socket_address).await;
        });
    }
}

fn get_json_str() -> String {
    let response = Json {
        company: "cluster43".to_string()
    };
    serde_json::to_string(&response).unwrap()
}

fn get_http_response_headers() -> String {
    let body_string = get_json_str();
    let content_length = body_string.len();

    format!("HTTP/1.1 200 OK\r\nDate: Mon, 27 Jul 2009 12:28:53 GMT\r\nServer: Apache/2.2.14 (Win32)\r\nLast-Modified: Wed, 22 Jul 2009 19:15:56 GMT\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: Closed\r\n\r\n{}",content_length,body_string)
}



async fn read_bytes_from_socket(read_half: &mut OwnedReadHalf) -> Result<Vec<u8>, std::io::Error> {
    let mut bytes_data = vec![0;1024];
    let mut ptr: usize = 0;
    let mut data_size = 0;
    loop {
        info!("Start reading Data from socket");
        let mut buf = [0; 256];
        let n: usize = match read_half.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => return Err(e)
        };
        if n <= 0 {
            break;
        }
        info!("Read {} bytes",n);
        data_size += n;
        let end_ptr = min(n, bytes_data.len()-data_size);
        for i in 0..end_ptr {
            bytes_data[ptr] = buf[i];
            ptr += 1;
        }
        if n < buf.len() {
            break;
        }
        info!("Copied Data");
    }
    info!("Data read Complete");
    Ok(bytes_data[..data_size].to_owned())
}

fn parse_http_request_bytes(bytes: &[u8]) -> Result<Request<()>,&'static str> {

    let mut headers = [EMPTY_HEADER;10];
    let mut http_parser_req = httparse::Request::new(&mut headers);
    match http_parser_req.parse(bytes) {
        Ok(status) => {
            match status {
                Status::Complete(headers) => {
                    info!("Number of headers: {:?}",headers);
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

    let mut hyper_request_builder = Request::builder()
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
            hyper_request_builder = hyper_request_builder.header(header_name, header_value);
        }
    }

    Ok(hyper_request_builder.body(()).unwrap())
}

async fn process(socket: TcpStream, _ip: SocketAddr)  {

    let ( mut read_half, mut write_half) = socket.into_split();

    let json_http_resp = get_http_response_headers();
    info!("Response: {}",json_http_resp);
    let http_resp_buff = json_http_resp.as_bytes();

    let bytes = read_bytes_from_socket(&mut read_half).await.unwrap();
    let header_map: Request<()> = parse_http_request_bytes(&bytes).unwrap();

    info!("Output received from client: {:?}",header_map);

     match write_half.write(http_resp_buff).await {
         Ok(n) => info!("Data sent size: {}",n),
         Err(e) => info!("Enable to send Data : {}",e)
     };

}
