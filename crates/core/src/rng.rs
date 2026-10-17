/// PCG-XSH-RR 32-bit generator. State lives in the save so restores repeat.
#[derive(Debug, Clone, Copy)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    pub fn new(seed: u64) -> Self {
        // Stream id is derived from the seed so a single stored seed is enough
        // to reconstruct a fresh generator. The full state is what saves store.
        let mut rng = Self {
            state: 0,
            inc: (seed << 1) | 1,
        };
        let _ = rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        let _ = rng.next_u32();
        rng
    }

    pub fn from_parts(state: u64, inc: u64) -> Self {
        Self {
            state,
            inc: inc | 1,
        }
    }

    pub fn state(self) -> u64 {
        self.state
    }

    pub fn inc(self) -> u64 {
        self.inc
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_matches() {
        let mut a = Pcg32::new(1);
        let mut b = Pcg32::new(1);
        for _ in 0..8 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        assert_ne!(Pcg32::new(1).next_u32(), Pcg32::new(2).next_u32());
    }

    #[test]
    fn restore_continues_sequence() {
        let mut a = Pcg32::new(99);
        let _ = a.next_u32();
        let mut b = Pcg32::from_parts(a.state(), a.inc());
        assert_eq!(a.next_u32(), b.next_u32());
    }
}
