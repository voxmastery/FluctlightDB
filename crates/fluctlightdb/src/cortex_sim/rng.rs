//! Seeded deterministic PRNG (xorshift64*).

#[derive(Debug, Clone, Copy)]
pub struct CortexRng {
    state: u64,
}

impl CortexRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9e37_79b9_7f4a_7c15 } else { seed },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub fn gen_range(&mut self, max_exclusive: u64) -> u64 {
        if max_exclusive == 0 {
            return 0;
        }
        self.next_u64() % max_exclusive
    }
}
