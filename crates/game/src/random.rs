use rand::Rng;
use rand::RngCore;
use rand::SeedableRng;
use rand::distr::uniform::SampleRange;
use rand::distr::uniform::SampleUniform;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct RandomState {
    pub seed: u64,
}
impl Default for RandomState {
    fn default() -> Self {
        RandomState::new()
    }
}

impl RandomState {
    pub fn new() -> RandomState {
        RandomState { seed: 42 }
    }

    /// Shuffles the provided slice in place and returns the new RandomState.
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        // Create a reproducible RNG from the current state's seed.
        let mut rng = ChaCha8Rng::seed_from_u64(self.seed);
        // Shuffle the slice using the RNG.
        slice.shuffle(&mut rng);
        // Update the state by consuming a value from the RNG.
        let new_seed = rng.next_u64();
        self.seed = new_seed;
    }

    /// Picks a random element from the options slice and returns it with the updated state.
    /// Note: `options` must be non-empty.
    pub fn pick<'a, T>(&mut self, options: &'a [T]) -> (usize, &'a T) {
        // Create a reproducible RNG from the current state's seed.
        let mut rng = ChaCha8Rng::seed_from_u64(self.seed);
        // Generate a random index.
        let index = rng.random_range(0..options.len());
        // Update the state.
        let new_seed = rng.next_u64();
        let rtn = &options[index];
        self.seed = new_seed;
        (index, rtn)
    }

    pub fn gen_range<T, R>(&mut self, range: R) -> T
    where
        T: SampleUniform,
        R: SampleRange<T>,
    {
        // Create a reproducible RNG from the current state's seed.
        let mut rng = ChaCha8Rng::seed_from_u64(self.seed);
        // Generate a random number in the provided range.
        let rtn = rng.random_range(range);
        // Update the state.
        let new_seed = rng.next_u64();
        self.seed = new_seed;
        rtn
    }
}
