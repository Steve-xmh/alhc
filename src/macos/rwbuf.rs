use std::fmt::Debug;

pub struct ReadWriteBuffer<const S: usize> {
    buf: [u8; S],
    read_len: usize,
    write_len: usize,
}

impl<const S: usize> Debug for ReadWriteBuffer<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ReadWriteBuffer")
            .field(&self.read_len)
            .field(&self.write_len)
            .finish()
    }
}

impl<const S: usize> Default for ReadWriteBuffer<S> {
    fn default() -> Self {
        Self {
            buf: [0u8; S],
            read_len: 0,
            write_len: 0,
        }
    }
}

impl<const S: usize> ReadWriteBuffer<S> {
    pub fn is_full(&self) -> bool {
        self.write_len == S
    }

    pub fn get_writable_slice(&mut self) -> &mut [u8] {
        &mut self.buf[self.write_len..]
    }

    pub fn increase_write_len(&mut self, len: usize) {
        self.write_len += len;
        debug_assert!(self.write_len <= self.buf.len());
    }

    pub fn get_writable_len(&self) -> usize {
        S - self.write_len
    }

    pub fn get_readable_slice(&self) -> &[u8] {
        &self.buf[self.read_len..self.write_len]
    }

    pub fn increase_read_len(&mut self, len: usize) {
        self.read_len += len;
        debug_assert!(self.read_len <= self.write_len);
        if self.read_len == self.write_len {
            self.read_len = 0;
            self.write_len = 0;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.read_len == 0 && self.write_len == 0
    }

    pub fn get_readable_len(&self) -> usize {
        self.write_len - self.read_len
    }

    pub fn is_readable(&self) -> bool {
        self.get_readable_len() > 0
    }
}
