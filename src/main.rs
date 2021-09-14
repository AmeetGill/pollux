use tokio::net::{TcpListener, TcpStream};
use std::net::SocketAddr;
use tokio::io::{ AsyncWriteExt};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::{Config};
use log::{info, LevelFilter};
use log4rs;
use log4rs::encode::pattern::{PatternEncoder};
use http::{Request, StatusCode};

mod httphandler;

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

async fn process(socket: TcpStream, _ip: SocketAddr)  {

    let ( mut read_half, mut write_half) = socket.into_split();
    let mut http_resp = httphandler::get_websocket_http_response(StatusCode::OK);
    info!("Response: {:?}",http_resp);
    let http_resp_bytes = httphandler::get_http_response_bytes(&mut http_resp);
    info!("Response: {:?}",http_resp_bytes);

    let bytes = httphandler::read_bytes_from_socket(&mut read_half).await.unwrap();
    let http_request: Request<()> = httphandler::parse_http_request_bytes(&bytes).unwrap();

    info!("Output received from client: {:?}",http_request);

     match write_half.write(&*http_resp_bytes).await {
         Ok(n) => info!("Data sent size: {}",n),
         Err(e) => info!("Enable to send Data : {}",e)
     };

}
