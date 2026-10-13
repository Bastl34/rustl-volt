// little endian binary reader

use anyhow::{bail, Result};

pub struct Reader
{
    data: Vec<u8>,
    pos: usize,
}

impl Reader
{
    pub fn new(data: Vec<u8>) -> Self
    {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize
    {
        self.data.len() - self.pos
    }

    fn bytes<const N: usize>(&mut self) -> Result<[u8; N]>
    {
        if self.remaining() < N
        {
            bail!("unexpected end of file at byte {}", self.pos);
        }

        let bytes = self.data[self.pos..self.pos + N].try_into()?;
        self.pos += N;
        Ok(bytes)
    }

    pub fn u8(&mut self) -> Result<u8> { Ok(u8::from_le_bytes(self.bytes()?)) }
    pub fn u16(&mut self) -> Result<u16> { Ok(u16::from_le_bytes(self.bytes()?)) }
    pub fn i16(&mut self) -> Result<i16> { Ok(i16::from_le_bytes(self.bytes()?)) }
    pub fn u32(&mut self) -> Result<u32> { Ok(u32::from_le_bytes(self.bytes()?)) }
    pub fn i32(&mut self) -> Result<i32> { Ok(i32::from_le_bytes(self.bytes()?)) }
    pub fn f32(&mut self) -> Result<f32> { Ok(f32::from_le_bytes(self.bytes()?)) }

    pub fn vec2(&mut self) -> Result<[f32; 2]>
    {
        Ok([self.f32()?, self.f32()?])
    }

    pub fn vec3(&mut self) -> Result<[f32; 3]>
    {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    // counts are stored signed - a negative one means a broken file
    pub fn count(&mut self) -> Result<usize>
    {
        let count = self.i32()?;
        if count < 0
        {
            bail!("negative count {count} at byte {}", self.pos - 4);
        }

        Ok(count as usize)
    }
}
