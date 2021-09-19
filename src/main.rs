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

extern crate base64;

mod http_handler;
mod error;
mod buffer;
mod data_frame;

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


    let bytes = http_handler::read_bytes_from_socket(&mut read_half).await.unwrap();
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

        if !data_frame.final_frame {
            // need to handle this
        }

        let mut data_bytes = http_handler::read_specified_bytes_from_socket(&mut read_half,data_frame.total_bytes_to_read).await.unwrap();

        info!("Message received from client: {}",
            data_frame::parse_data_to_string(
                &data_frame,
                &mut data_bytes
                )
        );


        if bytes.len() <= 0 {
            info!("No data received from client, closing connection");
            break;
        }
        let mut bytesss = "hell bro 😂".as_bytes().to_vec();
        let bytes_to_write = data_frame::create_text_frame(&bytesss);
        write_half.write(&bytes_to_write).await;
    }

    info!("Processing TcpStream: End");

}
