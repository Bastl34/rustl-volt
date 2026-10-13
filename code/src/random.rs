// xorshift seeded by the clock - enough for pauses and flight targets

#[derive(Default)]
pub struct Random
{
    state: u32,
}

impl Random
{
    // a fixed start - several of them made in the same instant would draw the same numbers
    pub fn seeded(seed: u32) -> Self
    {
        Self { state: seed | 1 }
    }

    // 0..1
    pub fn next(&mut self) -> f32
    {
        if self.state == 0
        {
            self.state = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()) | 1;
        }
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}
