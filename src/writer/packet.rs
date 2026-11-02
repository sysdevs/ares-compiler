//! Big-endian writer for the packs.

#[derive(Default, Debug)]
pub struct ByteWriter {
    buf: Vec<u8>,
}

impl ByteWriter {
    pub fn p1(&mut self, value: u8) {
        self.buf.push(value);
    }

    pub fn p2(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    pub fn p4(&mut self, value: i32) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    /// `< 32768` (signed) → the low 16 bits, else 4 bytes with the top bit set.
    pub fn p_smart2or4(&mut self, value: i32) {
        if value < 32768 {
            self.p2(value as u16);
        } else {
            self.p4(value | i32::MIN);
        }
    }

    pub fn pdata(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_big_endian_and_smart() {
        let mut w = ByteWriter::default();
        w.p1(0xff);
        w.p2(0xffff);
        w.p4(1);
        w.p_smart2or4(32767);
        w.p_smart2or4(32768);
        w.p_smart2or4(-2);
        assert_eq!(w.into_bytes(), [0xff, 0xff, 0xff, 0, 0, 0, 1, 0x7f, 0xff, 0x80, 0, 0x80, 0, 0xff, 0xfe]);
    }
}
