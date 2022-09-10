// In this case, a server MAY send a Close
// frame with a status code of 1002 (protocol error)
// A client MUST close a connection if it detects a masked
//    frame

// read frames in to text

use std::io::Read;
use crate::buffer::buffer::Buffer;
use crate::{tcp_handler, channel_handler};
use tokio::net::tcp::OwnedReadHalf;
use tokio::io::{AsyncReadExt, ReadBuf};
use tokio::sync::mpsc::Receiver;

static FIN_BITMASK: u8 =  0b10000000;
static RSV1_BITMASK: u8 = 0b01000000;
static RSV2_BITMASK: u8 = 0b00100000;
static RSV3_BITMASK: u8 = 0b00010000;
static OPCODE_BITMASK: u8 = 0b00001111;
static WEBSOCKET_MASK_BITMASK: u8 = 0b10000000;
static INITIAL_PAYLOAD_LENGTH_MASK: u8 = 0b01111111;

#[derive(Copy, Clone,Debug)]
pub enum Opcode {
    ContinuationFrame,
    TextFrame,
    BinaryFrame,
    ConnectionClose,
    Ping,
    Pong,
    NoOpcodeFound
}

#[derive(Copy, Clone,Debug)]
pub enum ReadFrom{
    Socket,
    Channel
}

static OPCODES_ARRAY: [(u8,Opcode);6] = [
    (0b00000000,Opcode::ContinuationFrame), // denotes a continuation frame
    (0b00000001,Opcode::TextFrame), // denotes a text frame
    (0b00000010,Opcode::BinaryFrame), // denotes a binary frame
    (0b00001000,Opcode::ConnectionClose), // denotes a connection close
    (0b00001001,Opcode::Ping), // denotes a ping
    (0b00001010,Opcode::Pong) // denotes a pong
];

#[derive(Debug)]
pub struct DataFrameInfo {
    pub mask_key: [u8;4],
    pub contain_masked_data: bool,
    pub payload_length: usize,
    pub payload_length_field: u8,
    pub opcode: Opcode,
    pub is_this_final_frame: bool,
    pub read_from: ReadFrom,
    pub raw_bytes: Vec<u8>,
    pub payload_data: Vec<u8>,
    pub rs1_bit_set: bool,
    pub rs2_bit_set: bool,
    pub rs3_bit_set: bool,
}


impl DataFrameInfo {
    
}

pub fn get_default_dataframe() -> DataFrameInfo {
    DataFrameInfo{
        mask_key: [0;4],
        contain_masked_data: false,
        payload_length: 0,
        payload_length_field: 0,
        opcode: Opcode::NoOpcodeFound,
        is_this_final_frame: false,
        read_from: ReadFrom::Socket,
        raw_bytes: Vec::new(),
        payload_data: Vec::new(),
        rs1_bit_set: false,
        rs2_bit_set: false,
        rs3_bit_set: false,
    }
}

// only supported on 64bit server
pub fn parse_data_to_string(data_frame_info: &DataFrameInfo, data: &mut [u8]) -> String {
    let str_res = "";

    if data_frame_info.contain_masked_data {
        mask_unmask_data(data,&data_frame_info.mask_key)
    }

    let str_res = std::str::from_utf8(data).unwrap();

    str_res.to_string()
}

/*
     0                   1                   2                   3
      0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
     +-+-+-+-+-------+-+-------------+-------------------------------+
     |F|R|R|R| opcode|M| Payload len |    Extended payload length    |
     |I|S|S|S|  (4)  |A|     (7)     |             (16/64)           |
     |N|V|V|V|       |S|             |   (if payload len==126/127)   |
     | |1|2|3|       |K|             |                               |
     +-+-+-+-+-------+-+-------------+ - - - - - - - - - - - - - - - +
     |     Extended payload length continued, if payload len == 127  |
     + - - - - - - - - - - - - - - - +-------------------------------+
     |                               |Masking-key, if MASK set to 1  |
     +-------------------------------+-------------------------------+
     | Masking-key (continued)       |          Payload Data         |
     +-------------------------------- - - - - - - - - - - - - - - - +
     :                     Payload Data continued ...                :
     + - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - +
     |                     Payload Data continued ...                |
     +---------------------------------------------------------------+
 */
pub async fn read_next_dataframe_from_socket(read_half: &mut OwnedReadHalf) -> DataFrameInfo {
    let mut data_frame_info = DataFrameInfo {
        mask_key: [0;4],
        payload_length: 0,
        payload_length_field: 0,
        opcode: Opcode::NoOpcodeFound,
        is_this_final_frame: false,
        contain_masked_data: false,
        read_from: ReadFrom::Socket,
        raw_bytes: Vec::new(),
        payload_data: Vec::new(),
        rs1_bit_set: false,
        rs2_bit_set: false,
        rs3_bit_set: false,
    };
    let first_byte = read_half.read_u8().await.unwrap();
    process_data_frame_first_byte(&mut data_frame_info,first_byte);

    let second_byte = read_half.read_u8().await.unwrap();
    process_data_frame_second_byte(&mut data_frame_info,second_byte);

    if data_frame_info.payload_length_field == 126 {
        data_frame_info.raw_bytes.append(&mut tcp_handler::read_specified_bytes_from_socket(read_half, 2).await.unwrap());
    }
    if data_frame_info.payload_length_field == 127 {
        data_frame_info.raw_bytes.append(&mut tcp_handler::read_specified_bytes_from_socket(read_half, 8).await.unwrap());
    }
    calculate_payload_length(&mut data_frame_info);

    if data_frame_info.contain_masked_data {
        let mut buffer = tcp_handler::read_specified_bytes_from_socket(read_half, 4).await.unwrap();
        data_frame_info.raw_bytes.append(&mut buffer);
    }
    extract_mask_key(&mut data_frame_info);

    data_frame_info.payload_data = tcp_handler::read_specified_bytes_from_socket(read_half, data_frame_info.payload_length).await.unwrap();
    data_frame_info
}

pub async fn read_dataframe_from_rx(rx: &mut Receiver<Vec<u8>>) -> DataFrameInfo {
    let mut data_frame_info = DataFrameInfo {
        mask_key: [0;4],
        payload_length: 0,
        payload_length_field: 0,
        opcode: Opcode::NoOpcodeFound,
        is_this_final_frame: false,
        contain_masked_data: false,
        read_from: ReadFrom::Channel,
        raw_bytes: Vec::new(),
        payload_data: Vec::new(),
        rs1_bit_set: false,
        rs2_bit_set: false,
        rs3_bit_set: false,
    };
    let vec_dataframe_bytes = rx.recv().await.unwrap();
    let mut ptr:usize = 0;
    let first_byte = vec_dataframe_bytes[ptr];
    ptr += 1;
    process_data_frame_first_byte(&mut data_frame_info,first_byte);

    let second_byte = vec_dataframe_bytes[ptr];
    ptr += 1;
    process_data_frame_second_byte(&mut data_frame_info,second_byte);

    if data_frame_info.payload_length_field == 126 {
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
    }
    if data_frame_info.payload_length_field == 127 {
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
    }
    calculate_payload_length(&mut data_frame_info);

    if data_frame_info.contain_masked_data {
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
        data_frame_info.raw_bytes.push(vec_dataframe_bytes[ptr]);
        ptr += 1;
    }
    data_frame_info.payload_data = channel_handler::read_vector_from_channel(rx).await.unwrap();
    data_frame_info
}

fn parse_opcode(opcode: u8) -> Opcode{
    for opcode_mapping in OPCODES_ARRAY.iter() {
        if opcode & opcode_mapping.0 != 0 {
            return opcode_mapping.1;
        }
    }
    Opcode::NoOpcodeFound
}

fn get_payload_length_bits(data_len: usize, mut masked_bit: u8) -> Vec<u8> {
    let mut vec: Vec<u8> = Vec::new();
    if data_len <= 125 {
        masked_bit |= data_len as u8;
        vec.push(masked_bit);
    } else {
        if data_len <= u16::MAX as usize {
            masked_bit |= 126;
            vec.push(masked_bit);
            vec.push((data_len >> 8) as u8 );
            vec.push(data_len as u8 );
        } else {
            masked_bit |= 127;
            vec.push(masked_bit);
            vec.push((data_len >> 56) as u8 );
            vec.push((data_len >> 48) as u8 );
            vec.push((data_len >> 40) as u8 );
            vec.push((data_len >> 32) as u8 );
            vec.push((data_len >> 24) as u8 );
            vec.push((data_len >> 16) as u8 );
            vec.push((data_len >> 8) as u8 );
            vec.push(data_len as u8 );
        }
    }
    vec
}

pub fn create_pong_frame(data_len: usize) -> Vec<u8> {
    let mut pong_frame = Buffer::new_unbound();
    let pong_opcode: u8 = 0b00001010;
    pong_frame.append_byte(0b10000000 | pong_opcode);
    let mut masked_bit: u8 = 0;

    pong_frame.append_vec8_array(
        &get_payload_length_bits(data_len,masked_bit)
    );

    pong_frame.get_arr()
}

pub fn mask_unmask_data(data: &mut [u8], mask_key: &[u8;4]) {
    for i in 0..data.len() {
        let mut j = i % 4;
        data[i] ^= mask_key[j];
    }
}

pub fn create_text_frame(data: &[u8]) -> Vec<u8> {
    let mut text_frame = Buffer::new_unbound();
    let text_opcode: u8 = 0b00000001;
    text_frame.append_byte(0b10000000 | text_opcode);
    let mut masked_bit: u8 = 0;

    text_frame.append_vec8_array(
        &get_payload_length_bits(data.len(),masked_bit)
    );

    text_frame.append_u8_array(data);

    text_frame.get_arr()
}

pub fn create_text_frame_with_data_length(data_len: usize) -> Vec<u8> {
    let mut text_frame = Buffer::new_unbound();
    let text_opcode = 0b10000001;
    text_frame.append_byte(text_opcode);
    let mut masked_bit: u8 = 0;

    text_frame.append_vec8_array(
        &get_payload_length_bits(data_len,masked_bit)
    );

    text_frame.get_arr()
}

pub fn create_binary_frame_with_data_length(data_len: usize) -> Vec<u8> {
    let mut text_frame = Buffer::new_unbound();
    let binary_opcode = 0b10000010;
    text_frame.append_byte(binary_opcode);
    let mut masked_bit: u8 = 0;

    text_frame.append_vec8_array(
        &get_payload_length_bits(data_len,masked_bit)
    );

    text_frame.get_arr()
}

fn process_data_frame_first_byte(data_frame_info: &mut DataFrameInfo, first_byte: u8) {
    data_frame_info.is_this_final_frame = ((first_byte & FIN_BITMASK) != 0);
    data_frame_info.raw_bytes.push(first_byte);

    //     RSV1 bit of the WebSocket header for
    //    PMCEs and calls the bit the "Per-Message Compressed" bit.  On a
    //    WebSocket connection where a PMCE is in use, this bit indicates
    //    whether a message is compressed or not.
    data_frame_info.rs1_bit_set = RSV1_BITMASK & first_byte != 0;
    data_frame_info.rs2_bit_set = RSV2_BITMASK & first_byte != 0;
    data_frame_info.rs3_bit_set = RSV3_BITMASK & first_byte != 0;

    let opcode = OPCODE_BITMASK & first_byte;
    data_frame_info.opcode = parse_opcode(opcode);
}

fn process_data_frame_second_byte(data_frame_info: &mut DataFrameInfo, second_byte: u8)  {
    data_frame_info.raw_bytes.push(second_byte);
    let websocket_mask_set = WEBSOCKET_MASK_BITMASK & second_byte != 0;
    data_frame_info.contain_masked_data = websocket_mask_set;
    data_frame_info.payload_length_field = INITIAL_PAYLOAD_LENGTH_MASK & second_byte
}

fn calculate_payload_length(data_frame_info: &mut DataFrameInfo) {
    let mut payload_length: usize = 0;
    if data_frame_info.payload_length_field <= 125 {
        payload_length = data_frame_info.payload_length_field as usize;
    }

    if data_frame_info.payload_length_field == 126 {
        payload_length |= (data_frame_info.raw_bytes[2] as usize) << 8;
        payload_length |=  data_frame_info.raw_bytes[3] as usize;
    }
    if data_frame_info.payload_length_field == 127 {
        payload_length |= (data_frame_info.raw_bytes[2] as usize) << 56;
        payload_length |= (data_frame_info.raw_bytes[3] as usize) << 48;
        payload_length |= (data_frame_info.raw_bytes[4] as usize) << 40;
        payload_length |= (data_frame_info.raw_bytes[5] as usize) << 32;
        payload_length |= (data_frame_info.raw_bytes[6] as usize) << 24;
        payload_length |= (data_frame_info.raw_bytes[7] as usize) << 16;
        payload_length |= (data_frame_info.raw_bytes[8] as usize) << 8;
        payload_length |= data_frame_info.raw_bytes[9] as usize
    }

    data_frame_info.payload_length = payload_length;
}

fn extract_mask_key(data_frame_info: &mut DataFrameInfo) {
    let len = data_frame_info.raw_bytes.len();
    let mut mask_key:[u8;4] = [0;4];
    if data_frame_info.contain_masked_data {
        mask_key[0] = data_frame_info.raw_bytes[len - 4];
        mask_key[1] = data_frame_info.raw_bytes[len - 3];
        mask_key[2] = data_frame_info.raw_bytes[len - 2];
        mask_key[3] = data_frame_info.raw_bytes[len - 1];
        data_frame_info.mask_key = mask_key.clone();
    }
}

