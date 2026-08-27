use std::collections::HashMap;

use md5::{Digest, Md5};

use crate::{program::RandomSequenceSettings, resource::Identifier};

const GOLDEN_RATIO_64: u64 = 0x9e37_79b9_7f4a_7c15;
const SILVER_RATIO_64: u64 = 0x6a09_e667_f3bc_c909;

#[derive(Debug)]
pub(crate) struct RandomState {
    unnamed: LegacyRandom,
    sequences: RandomSequences,
}

impl RandomState {
    pub(crate) fn new(world_seed: i64) -> Self {
        Self {
            unnamed: LegacyRandom::default(),
            sequences: RandomSequences::new(world_seed),
        }
    }

    pub(crate) fn world_seed(&self) -> i64 {
        self.sequences.world_seed()
    }

    pub(crate) fn unnamed(&mut self) -> &mut LegacyRandom {
        &mut self.unnamed
    }

    pub(crate) fn parts(&mut self) -> (&mut LegacyRandom, &mut RandomSequences) {
        (&mut self.unnamed, &mut self.sequences)
    }
}

#[derive(Debug)]
pub(crate) struct LegacyRandom {
    seed: u64,
}

impl LegacyRandom {
    const DEFAULT_SEED: i64 = 0;
    const MASK: u64 = (1_u64 << 48) - 1;
    const MULTIPLIER: u64 = 25_214_903_917;
    const INCREMENT: u64 = 11;

    fn new(seed: i64) -> Self {
        Self {
            seed: ((seed as u64) ^ Self::MULTIPLIER) & Self::MASK,
        }
    }

    fn next_bits(&mut self, bits: u32) -> i32 {
        self.seed = self
            .seed
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::INCREMENT)
            & Self::MASK;
        (self.seed >> (48 - bits)) as i32
    }

    pub(crate) fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 * 5.960_464_5e-8
    }

    pub(crate) fn next_int(&mut self, bound: i32) -> Result<i32, String> {
        if bound <= 0 {
            return Err("random integer bound must be positive".to_owned());
        }
        if bound & bound.wrapping_sub(1) == 0 {
            return Ok(((i64::from(bound) * i64::from(self.next_bits(31))) >> 31) as i32);
        }
        loop {
            let sample = self.next_bits(31);
            let modulo = sample % bound;
            if sample.wrapping_sub(modulo).wrapping_add(bound - 1) >= 0 {
                return Ok(modulo);
            }
        }
    }
}

impl Default for LegacyRandom {
    fn default() -> Self {
        // Minecraft leaves each level's initial random seed unspecified. The
        // compatibility contract requires Worldless to choose such results
        // deterministically for identical VM inputs and state.
        Self::new(Self::DEFAULT_SEED)
    }
}

#[derive(Debug)]
pub(crate) struct RandomSequences {
    world_seed: i64,
    defaults: RandomSequenceSettings,
    sequences: HashMap<Identifier, XoroshiroRandomSource>,
}

impl RandomSequences {
    pub(crate) fn new(world_seed: i64) -> Self {
        Self {
            world_seed,
            defaults: RandomSequenceSettings::minecraft_default(0),
            sequences: HashMap::new(),
        }
    }

    pub(crate) fn world_seed(&self) -> i64 {
        self.world_seed
    }

    pub(crate) fn materialize(&mut self, id: &Identifier) {
        if self.sequences.contains_key(id) {
            return;
        }
        let source = self.create_sequence(id, &self.defaults);
        self.sequences.insert(id.clone(), source);
    }

    pub(crate) fn next_int(&mut self, id: &Identifier, bound: i32) -> i32 {
        self.materialize(id);
        self.sequences
            .get_mut(id)
            .expect("the named random sequence was materialized")
            .next_int(bound)
    }

    pub(crate) fn reset(&mut self, id: Identifier, settings: Option<RandomSequenceSettings>) {
        let source = self.create_sequence(&id, settings.as_ref().unwrap_or(&self.defaults));
        self.sequences.insert(id, source);
    }

    pub(crate) fn clear(&mut self) -> usize {
        let count = self.sequences.len();
        self.sequences.clear();
        count
    }

    pub(crate) fn set_defaults_and_clear(&mut self, settings: RandomSequenceSettings) -> usize {
        self.defaults = settings;
        self.clear()
    }

    fn create_sequence(
        &self,
        id: &Identifier,
        settings: &RandomSequenceSettings,
    ) -> XoroshiroRandomSource {
        let world_seed = if settings.include_world_seed {
            self.world_seed
        } else {
            0
        };
        let seed = (world_seed as u64) ^ (settings.salt as i64 as u64);
        let mut upgraded = upgrade_seed_to_128bit_unmixed(seed);
        if settings.include_sequence_id {
            upgraded = upgraded.xor(seed_for_key(id));
        }
        XoroshiroRandomSource::new(upgraded.mixed())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Seed128bit {
    low: u64,
    high: u64,
}

impl Seed128bit {
    fn xor(self, other: Self) -> Self {
        Self {
            low: self.low ^ other.low,
            high: self.high ^ other.high,
        }
    }

    fn mixed(self) -> Self {
        Self {
            low: mix_stafford_13(self.low),
            high: mix_stafford_13(self.high),
        }
    }
}

fn mix_stafford_13(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn upgrade_seed_to_128bit_unmixed(seed: u64) -> Seed128bit {
    let low = seed ^ SILVER_RATIO_64;
    Seed128bit {
        low,
        high: low.wrapping_add(GOLDEN_RATIO_64),
    }
}

fn seed_for_key(id: &Identifier) -> Seed128bit {
    let digest = Md5::digest(id.to_string().as_bytes());
    Seed128bit {
        low: u64::from_be_bytes(
            digest[..8]
                .try_into()
                .expect("an MD5 digest has sixteen bytes"),
        ),
        high: u64::from_be_bytes(
            digest[8..]
                .try_into()
                .expect("an MD5 digest has sixteen bytes"),
        ),
    }
}

#[derive(Debug)]
struct Xoroshiro128PlusPlus {
    low: u64,
    high: u64,
}

impl Xoroshiro128PlusPlus {
    fn new(seed: Seed128bit) -> Self {
        let (low, high) = if seed.low | seed.high == 0 {
            (GOLDEN_RATIO_64, SILVER_RATIO_64)
        } else {
            (seed.low, seed.high)
        };
        Self { low, high }
    }

    fn next_long(&mut self) -> u64 {
        let low = self.low;
        let mut high = self.high;
        let result = low.wrapping_add(high).rotate_left(17).wrapping_add(low);
        high ^= low;
        self.low = low.rotate_left(49) ^ high ^ high.wrapping_shl(21);
        self.high = high.rotate_left(28);
        result
    }
}

#[derive(Debug)]
struct XoroshiroRandomSource {
    generator: Xoroshiro128PlusPlus,
}

impl XoroshiroRandomSource {
    fn new(seed: Seed128bit) -> Self {
        Self {
            generator: Xoroshiro128PlusPlus::new(seed),
        }
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "random integer bound must be positive");
        let unsigned_bound = bound as u32;
        let threshold = unsigned_bound.wrapping_neg() % unsigned_bound;
        loop {
            let random_bits = self.generator.next_long() as u32;
            let product = u64::from(random_bits) * u64::from(unsigned_bound);
            if product as u32 >= threshold {
                return (product >> 32) as i32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> Identifier {
        Identifier::parse(value).unwrap()
    }

    fn settings(
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) -> RandomSequenceSettings {
        RandomSequenceSettings {
            salt,
            include_world_seed,
            include_sequence_id,
        }
    }

    fn next_longs(sequences: &mut RandomSequences, id: &Identifier, count: usize) -> Vec<u64> {
        sequences.materialize(id);
        let source = sequences.sequences.get_mut(id).unwrap();
        (0..count).map(|_| source.generator.next_long()).collect()
    }

    #[test]
    fn stafford_mix_matches_minecraft_vectors() {
        assert_eq!(mix_stafford_13(0), 0);
        assert_eq!(mix_stafford_13(1), 0x5692_161d_100b_05e5);
        assert_eq!(mix_stafford_13(u64::MAX), 0xb4d0_55fc_f2cb_bd7b);
        assert_eq!(mix_stafford_13(i64::MIN as u64), 0x25c2_6ea5_79ce_a98a);
    }

    #[test]
    fn identifier_hash_uses_canonical_utf8_md5_big_endian_halves() {
        assert_eq!(
            seed_for_key(&id("test")),
            Seed128bit {
                low: 0x1a2b_64f5_d3bd_b461,
                high: 0x2e96_d0a4_22fe_68a7,
            }
        );
        assert_eq!(
            seed_for_key(&id("test:sequence")),
            Seed128bit {
                low: 0xb18a_7107_f06b_b634,
                high: 0x2936_bb46_3e4b_763e,
            }
        );
    }

    #[test]
    fn xoroshiro_state_transition_and_zero_fallback_match_minecraft() {
        let mut generator = Xoroshiro128PlusPlus::new(Seed128bit { low: 1, high: 2 });
        assert_eq!(
            (0..5).map(|_| generator.next_long()).collect::<Vec<_>>(),
            [
                0x0000_0000_0006_0001,
                0x0002_60c0_0066_0007,
                0x180a_cc04_7186_06d3,
                0x9e22_6d35_036f_c4c7,
                0x849b_c9ac_6b96_0be4,
            ]
        );

        let mut zero = Xoroshiro128PlusPlus::new(Seed128bit { low: 0, high: 0 });
        assert_eq!(zero.next_long(), 0x5e7a_5fc8_0986_8c97);
        assert_eq!(zero.next_long(), 0x4935_93f7_4710_0caf);
    }

    #[test]
    fn bounded_integer_rejection_consumes_each_rejected_sample() {
        let mut random = XoroshiroRandomSource::new(Seed128bit { low: 1, high: 2 });
        assert_eq!(random.next_int(1_500_000_001), 137_329);
        assert_eq!(random.next_int(1_500_000_001), 20_136_306);
        assert_eq!(random.generator.next_long(), 0x849b_c9ac_6b96_0be4);
    }

    #[test]
    fn named_sequences_match_world_seed_salt_and_id_vectors() {
        let sequence = id("minecraft:test");
        let mut defaults = RandomSequences::new(0);
        assert_eq!(
            next_longs(&mut defaults, &sequence, 4),
            [
                0x24ff_62b4_c7f4_7d70,
                0x011a_b06d_cec7_39ba,
                0xe226_b18c_1d4b_d48b,
                0x6038_92b3_16ea_9d20,
            ]
        );

        let sequence = id("test:sequence");
        let mut negative_salt = RandomSequences::new(1_234_567_890_123_456_789);
        negative_salt.set_defaults_and_clear(settings(-123_456_789, true, true));
        assert_eq!(
            next_longs(&mut negative_salt, &sequence, 4),
            [
                0x4d49_5553_619b_f168,
                0x71df_415d_b542_c418,
                0x7caf_1147_062e_8330,
                0xcd66_e71e_42a7_c5ce,
            ]
        );

        let mut no_id = RandomSequences::new(1_234_567_890_123_456_789);
        no_id.set_defaults_and_clear(settings(-123_456_789, true, false));
        assert_eq!(
            next_longs(&mut no_id, &sequence, 4),
            [
                0x1c42_69c4_2d2e_07f7,
                0xc6bf_94c5_2988_59a6,
                0x0ba5_b3fd_e80e_80c1,
                0xc965_90ca_6f1a_f003,
            ]
        );
    }

    #[test]
    fn reset_and_clear_follow_minecraft_state_semantics() {
        let first = id("example:first");
        let second = id("example:second");
        let mut sequences = RandomSequences::new(0);
        let initial = next_longs(&mut sequences, &first, 1)[0];
        next_longs(&mut sequences, &second, 1);

        sequences.reset(first.clone(), None);
        assert_eq!(next_longs(&mut sequences, &first, 1), [initial]);
        assert_eq!(sequences.clear(), 2);
        assert_eq!(sequences.clear(), 0);

        sequences.reset(first.clone(), None);
        assert_eq!(
            sequences.set_defaults_and_clear(settings(7, false, false)),
            1
        );
        assert!(sequences.sequences.is_empty());
        sequences.materialize(&first);
    }

    #[test]
    fn explicit_reset_does_not_change_global_defaults() {
        let explicit = id("example:explicit");
        let lazy = id("example:lazy");
        let mut sequences = RandomSequences::new(0);
        sequences.reset(explicit, Some(settings(9, false, false)));
        sequences.clear();

        let actual = next_longs(&mut sequences, &lazy, 1)[0];
        let mut expected = RandomSequences::new(0);
        assert_eq!(actual, next_longs(&mut expected, &lazy, 1)[0]);
    }
}
