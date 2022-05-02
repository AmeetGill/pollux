use std::net::SocketAddr;
use log::{info, error, LevelFilter};
use log4rs::Config;
use log4rs;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use http::{Request, StatusCode};
use crate::data_frame::{Opcode, DataFrameInfo, ReadFrom};
use crate::model::{Message};
use crate::service_config::{ServiceConfig};
use tokio::sync::{mpsc, Mutex};
use tokio::net::tcp::{OwnedReadHalf,OwnedWriteHalf};
use tokio::sync::mpsc::Receiver;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;
use redis_client::{RedisClient};

#[macro_use]
extern crate lazy_static;
extern crate base64;
extern crate redis;

mod http_handler;
mod error;
mod buffer;
mod data_frame;
mod model;
mod tcp_handler;
mod redis_client;
mod channel_handler;
mod workers;
mod service_config;

//           ws-URI = "ws:" "//" host [ ":" port ] path [ "?" query ]
//           wss-URI = "wss:" "//" host [ ":" port ] path [ "?" query ]
//
//           host = <host, defined in [RFC3986], Section 3.2.2>
//           port = <port, defined in [RFC3986], Section 3.2.3>
//           path = <path-abempty, defined in [RFC3986], Section 3.3>
//           query = <query, defined in [RFC3986], Section 3.4>
//            The port component is OPTIONAL; the default for "ws" is port 80,
//           while the default for "wss" is port 443.

lazy_static! {
    static ref USER_ID_MAPPING: Mutex<HashMap<String,Sender<Vec<u8>>>> = {
        let mut m = Mutex::new(HashMap::new());
        m
    };
    static ref REDIS_CLIENT: Mutex<Option<RedisClient>> = {
        Mutex::new(None)
    };
    static ref MY_ADDRESS: String = {
        format!("{}{}","127.0.0.1:",rand::random::<u16>())
    };
    static ref TCP_WORKER_ADDRESS: String = {
        format!("{}{}","127.0.0.1:",rand::random::<u16>())
    };
    static ref SERVICE_CONFIG: ServiceConfig = {
        let mut m = service_config::get_default_config();
        m
    };
}

#[tokio::main]
async fn main() {

    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {h({l})} {T} [{f:1.10}:{L}] [{M}] [] {m}{n}")))
        .build();

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(LevelFilter::Trace))
        .unwrap();

    let _handle = log4rs::init_config(config).unwrap();
    info!("My Address, {}",MY_ADDRESS.to_ascii_lowercase());
    // Bind the listener to the address
    let listener = TcpListener::bind(MY_ADDRESS.to_ascii_lowercase()).await.unwrap();

    if SERVICE_CONFIG.cluster_mode {
        match RedisClient::initialize_redis_connection().await {
            Ok(redis_client) => {
                REDIS_CLIENT.lock().await.insert(redis_client);
            }
            Err(_) => {
                panic!("Not able to connect to redis_client");
            }
        }
        tokio::spawn(async move {
            workers::tcp_message_listener::listen_for_messages_from_other_services(TCP_WORKER_ADDRESS.to_ascii_lowercase()).await;
        });
    }

    loop {
        // The second item contains the IP and port of the new connection.
        info!("Waiting for Clients at Address: {}",MY_ADDRESS.to_ascii_lowercase());
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
async fn process(socket: TcpStream, _ip: SocketAddr )  {
    info!("Processing TcpStream: Start");
    let ( mut read_half, mut write_half) = socket.into_split();

    let bytes = tcp_handler::read_bytes_from_socket(&mut read_half).await.unwrap();
    println!("{}", std::str::from_utf8(&bytes).unwrap());
    let http_request: Request<()> = http_handler::parse_http_request_bytes(bytes).unwrap();

    // handshake
    let (http_resp,user_id) = http_handler::create_websocket_response(http_request).unwrap_or_else(|_error| {
        return (http_handler::create_401_response(),"".to_owned());
    });
    let http_response_status = http_resp.status().clone();
    let http_resp_bytes = http_handler::get_http_response_bytes(http_resp);
    match write_half.write(&*http_resp_bytes.unwrap()).await {
        Ok(n) => info!("Data sent size: {}",n),
        Err(e) => {
            error!("Enable to send Data : {}",e);
            return;
        }
    };
    if http_response_status != StatusCode::SWITCHING_PROTOCOLS{
        info!("Connection Unsuccessful, Dropping thread");
        return;
    }

    let (tx, mut rx) = mpsc::channel(100);
    USER_ID_MAPPING.lock().await.insert(user_id.clone(),tx.clone());
    if SERVICE_CONFIG.cluster_mode {
        REDIS_CLIENT.lock().await.as_mut().unwrap().set(user_id.clone(),TCP_WORKER_ADDRESS.to_ascii_lowercase()).unwrap();
    }

    loop {
        let mut data_frame = data_frame::get_default_dataframe();
        tokio::select! {
            val = data_frame::read_next_dataframe_from_socket(&mut read_half) => {
                data_frame = val;
            }
            val = data_frame::read_dataframe_from_rx(&mut rx) => {
                data_frame = val;
            }
        }
        info!("Reading Payload");
        // let mut payload_data = read_data(&data_frame,&mut rx,&mut read_half).await;
        // data_frame.payload_data = payload_data;
        info!(" ------------------------ Message processing start! ------------------------ ");
        let (close_connection, vec_to_send) = process_frame(&data_frame, &mut rx, &mut read_half).await;
        if close_connection {
            break;
        }
        info!("Length of vector payload {}",vec_to_send[1].len());
        let message: Message = serde_json::from_slice(&vec_to_send[1][..]).unwrap();
        info!("User-id of receiver {}",message.sender_user_id);
        send_response_frame(&data_frame, vec_to_send, message, &mut write_half).await;
        info!(" ---------------------------- Message processed! ---------------------------- ");
    }

    info!("Removing entry from hashMap");
    USER_ID_MAPPING.lock().await.remove(&user_id);
    if SERVICE_CONFIG.cluster_mode {
        info!("Removing entry from Redis: End");
        REDIS_CLIENT.lock().await.as_mut().unwrap().delete(user_id);
    }
    info!("Processing TcpStream: End");
}

async fn process_frame(data_frame: &DataFrameInfo,rx: &mut Receiver<Vec<u8>>, read_half: &mut OwnedReadHalf) -> (bool,Vec<Vec<u8>>) {
    return match data_frame.opcode {
        Opcode::TextFrame => {
            (false, process_text_frame(data_frame, rx, read_half).await)
        }
        Opcode::Ping => {
            (false, process_ping_frame(data_frame, rx, read_half).await)
        }
        Opcode::BinaryFrame => {
            (false, process_binary_frame(data_frame, rx, read_half).await)
        }
        Opcode::ConnectionClose => {
            info!("Close Connection Opcode received");
            (true, Vec::new() )
        }
        _ => {
            error!("Opcode not supported");
            (true, Vec::new())
        }
    };
}

async fn process_text_frame(data_frame: &DataFrameInfo,rx: &mut Receiver<Vec<u8>>, read_half: &mut OwnedReadHalf) -> Vec<Vec<u8>> {
    let mut vec_to_send: Vec<Vec<u8>> = Vec::new();

    info!("TextFrame: {:?}",data_frame);
    let mut text_data = read_data(&data_frame,rx,read_half).await;
    match data_frame.read_from {
        ReadFrom::Socket => {
            data_frame::mask_unmask_data(&mut text_data, &data_frame.mask_key);
            vec_to_send.push(data_frame::create_text_frame_with_data_length(text_data.len()));
            vec_to_send.push(text_data);
        }
        ReadFrom::Channel => {
            info!("message received from Channel");
            vec_to_send.push(data_frame.raw_bytes.clone());
            vec_to_send.push(text_data);
        }
    }
    return vec_to_send
}

async fn process_binary_frame(data_frame: &DataFrameInfo,rx: &mut Receiver<Vec<u8>>, read_half: &mut OwnedReadHalf) -> Vec<Vec<u8>> {
    let mut vec_to_send: Vec<Vec<u8>> = Vec::new();

    let mut binary_data = read_data(&data_frame,rx,read_half).await;
    match data_frame.read_from {
        ReadFrom::Socket => {
            data_frame::mask_unmask_data(&mut binary_data, &data_frame.mask_key);
            vec_to_send.push(data_frame::create_binary_frame_with_data_length(binary_data.len()));
            vec_to_send.push(binary_data);
        }
        ReadFrom::Channel => {
            info!("message received from Channel");
            vec_to_send.push(data_frame.raw_bytes.clone());
            vec_to_send.push(binary_data);
        }
    }

    return vec_to_send
}

async fn process_ping_frame(data_frame: &DataFrameInfo,rx: &mut Receiver<Vec<u8>>, read_half: &mut OwnedReadHalf) -> Vec<Vec<u8>> {
    let mut vec_to_send: Vec<Vec<u8>> = Vec::new();
    let mut ping_data = read_data(&data_frame,rx,read_half).await;
    data_frame::mask_unmask_data(&mut ping_data, &data_frame.mask_key);
    vec_to_send.push(data_frame::create_pong_frame(ping_data.len()));
    vec_to_send.push(ping_data);
    return vec_to_send
}

async fn send_response_frame(data_frame: &DataFrameInfo, vec_to_send: Vec<Vec<u8>>, message: Message, write_half: &mut OwnedWriteHalf) {
    match data_frame.read_from {
        ReadFrom::Socket => {
            match data_frame.opcode {
                Opcode::Ping => send_reply_arrived_to_this_user(vec_to_send,write_half).await,
                _ => {
                    match USER_ID_MAPPING.lock().await.get(&message.sender_user_id) {
                        None => send_dataframe_to_other_service(message,vec_to_send).await,
                        Some(tx2) => send_dataframe_to_channel(vec_to_send,tx2).await
                    }
                }
            }
        }
        ReadFrom::Channel => send_reply_arrived_to_this_user(vec_to_send,write_half).await
    };
}

async fn send_dataframe_to_other_service(message: Message,vec_to_send: Vec<Vec<u8>> ){
    if SERVICE_CONFIG.cluster_mode {
        info!("User not connected to this server");
        let ip: String = REDIS_CLIENT.lock().await.as_mut().unwrap().get(&message.sender_user_id).unwrap_or_else(|_err| {
            "".to_string()
        });
        info!("User connected to the server: {}",ip);

        if ip.len() > 0 {
            workers::tcp_message_transmitter::transmit(vec_to_send, ip).await;
        } else {
            error!("User Not Connected to any Service")
        }
    } else {
        info!("User Not Connected to this Service")
    }
}

async fn send_dataframe_to_channel(vec_to_send: Vec<Vec<u8>>, tx2: &Sender<Vec<u8>>){
    info!("Sending message to channel");
    let tx2 = tx2.clone();
    for bytes in vec_to_send {
        tx2.send(bytes).await.unwrap();
    }
    info!("Message Sent");
}

async fn send_reply_arrived_to_this_user(vec_to_send: Vec<Vec<u8>>, write_half: &mut OwnedWriteHalf ){
    info!("Sending reply arrived for this user");
    for vec_data in vec_to_send {
        write_half.write(&vec_data).await.unwrap();
    }
    info!("Reply Sent");
}


async fn read_data(data_frame: &DataFrameInfo, rx: &mut Receiver<Vec<u8>>, read_half: &mut OwnedReadHalf) -> Vec<u8>{
    match data_frame.read_from {
        ReadFrom::Socket => {
            tcp_handler::read_specified_bytes_from_socket(read_half, data_frame.total_bytes_to_read).await.unwrap()
        }
        ReadFrom::Channel => {
            channel_handler::read_vector_from_channel(rx).await.unwrap()
        }
    }
}
