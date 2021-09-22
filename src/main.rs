use std::net::SocketAddr;
use log::{info, error, LevelFilter};
use log4rs::Config;
use log4rs;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use http::{Request, Response, StatusCode};
use crate::data_frame::Opcode;

extern crate base64;
extern crate redis;

mod http_handler;
mod error;
mod buffer;
mod data_frame;
mod model;
mod tcp_handler;
mod redis_client;

//           ws-URI = "ws:" "//" host [ ":" port ] path [ "?" query ]
//           wss-URI = "wss:" "//" host [ ":" port ] path [ "?" query ]
//
//           host = <host, defined in [RFC3986], Section 3.2.2>
//           port = <port, defined in [RFC3986], Section 3.2.3>
//           path = <path-abempty, defined in [RFC3986], Section 3.3>
//           query = <query, defined in [RFC3986], Section 3.4>
//            The port component is OPTIONAL; the default for "ws" is port 80,
//           while the default for "wss" is port 443.

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

    let redis_connection = redis_client::get_redis_connection()
        .expect("Redis client connection failed");

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
// The server MUST close the connection upon receiving a
//    frame that is not masked. In this case, a server MAY send a Close
//    frame with a status code of 1002 (protocol error)
async fn process(socket: TcpStream, _ip: SocketAddr)  {
    info!("Processing TcpStream: Start");
    let ( mut read_half, mut write_half) = socket.into_split();

    let bytes = tcp_handler::read_bytes_from_socket(&mut read_half).await.unwrap();
    println!("{}", std::str::from_utf8(&bytes).unwrap());
    let http_request: Request<()> = http_handler::parse_http_request_bytes(&bytes).unwrap();

    // handshake
    let mut http_resp = http_handler::create_websocket_response(&http_request).unwrap_or_else(|_error| {
        return http_handler::create_401_response();
    });
    let http_response_status = http_resp.status().clone();

    let http_resp_bytes = http_handler::get_http_response_bytes(&mut http_resp);
    match write_half.write(&*http_resp_bytes.unwrap()).await {
        Ok(n) => info!("Data sent size: {}",n),
        Err(e) => error!("Enable to send Data : {}",e)
    };

    if http_response_status != StatusCode::SWITCHING_PROTOCOLS{
        info!("Connection Unsuccessful, Dropping thread");
        return;
    }

    loop {
        let mut data_frame = data_frame::read_next_websocket_dataframe(&mut read_half).await;
        let mut data_to_send: Option<Vec<Vec<u8>>> = None;
        match data_frame.opcode  {
            Opcode::TextFrame => {
                info!("TextFrame: {:?}",data_frame);
                let mut vec_to_send: Vec<Vec<u8>> = Vec::new();
                let mut text_data = tcp_handler::read_specified_bytes_from_socket(&mut read_half, data_frame.total_bytes_to_read).await.unwrap();
                data_frame::mask_unmask_data(&mut text_data,&data_frame.mask_key);
                vec_to_send.push(data_frame::create_text_frame_with_data_length(text_data.len()));
                vec_to_send.push(text_data);
                data_to_send = Some(vec_to_send);
            }
            Opcode::Ping => {
                let mut ping_data = tcp_handler::read_specified_bytes_from_socket(&mut read_half, data_frame.total_bytes_to_read).await.unwrap();
                data_frame::mask_unmask_data(&mut ping_data,&data_frame.mask_key);
                let mut vec_to_send: Vec<Vec<u8>> = Vec::new();
                vec_to_send.push(data_frame::create_pong_frame(ping_data.len()));
                vec_to_send.push(ping_data);
                data_to_send = Some(vec_to_send);
            }
            Opcode::BinaryFrame => {
                let mut vec_to_send: Vec<Vec<u8>> = Vec::new();
                let mut binary_data = tcp_handler::read_specified_bytes_from_socket(&mut read_half, data_frame.total_bytes_to_read).await.unwrap();
                data_frame::mask_unmask_data(&mut binary_data,&data_frame.mask_key);
                vec_to_send.push(data_frame::create_binary_frame_with_data_length(binary_data.len()));
                vec_to_send.push(binary_data);
                data_to_send = Some(vec_to_send);
            }
            Opcode::ConnectionClose => {
                info!("Close Connection Opcode received");
                break;
            }
            _ => {
                error!("Opcode not supported");
                break;
            }
        }
        if data_to_send.is_some() {
            for vec_data in data_to_send.unwrap() {
                write_half.write(&vec_data).await;
            }
        }
    }

    info!("Processing TcpStream: End");

}
