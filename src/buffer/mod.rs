pub const EMPTY_BYTE: u8 = 0;

pub struct Buffer<T> {
    curr_pos: usize,
    buffer_size: usize,
    buffer_arr: Vec<T>
}

impl Buffer<u8> {
    pub fn new(buff_size: usize) -> Box<Buffer<u8>> {
        Box::new(
            Buffer{
                curr_pos: 0,
                buffer_size: buff_size,
                buffer_arr: vec![EMPTY_BYTE;buff_size]
            }
        )
    }
    pub fn new_unbound() -> Box<Buffer<u8>> {
        Box::new(
            Buffer{
                curr_pos: 0,
                buffer_size: usize::MAX,
                buffer_arr: Vec::new()
            }
        )
    }
    pub fn append_byte(&mut self, byte: u8) -> Result<(),&'static str> {
        if self.curr_pos == self.buffer_size {
            return Err("Not enough space in buffer");
        }
        if self.buffer_arr.len() == self.curr_pos {
            self.buffer_arr.push(byte)
        } else {
            self.buffer_arr[self.curr_pos] = byte;
        }
        self.curr_pos += 1;
        Ok(())
    }

    pub fn can_array_be_appended(&self,array_size: usize) -> bool {
        self.curr_pos + array_size == self.buffer_size
    }

    pub fn append_u8_array(&mut self, u8_arr: &[u8]) -> Result<(),&'static str> {
        if self.can_array_be_appended(u8_arr.len()) {
            return Err("Not enough space in buffer");
        }
        for u8_byte in u8_arr {
            self.append_byte(*u8_byte)?;
        }
        Ok(())
    }

    pub fn append_vec8_array(&mut self, vec8_arr: &Vec<u8>) -> Result<(),&'static str> {
        if self.can_array_be_appended(vec8_arr.len()) {
            return Err("Not enough space in buffer");
        }
        for u8_byte in vec8_arr {
            self.append_byte(*u8_byte)?;
        }
        Ok(())
    }

    pub fn get_arr(self) -> Vec<u8> {
        self.buffer_arr[..self.curr_pos].to_vec()
    }

}
