pub struct PartialHasher<'a> {
    buf: &'a [u8],
    first: bool,

    current_crc: u32,
    rolling_bit_masks: [u32; 8],
}

impl<'a> PartialHasher<'a> {
    fn new_common(buf: &'a [u8]) -> Self {
        if buf.is_empty() {
            let crc = crc32fast::hash(b"");
            return Self {
                buf,
                first: true,
                current_crc: crc,
                rolling_bit_masks: [0; 8],
            };
        }

        let buf_size_u64 = u64::try_from(buf.len()).expect("u64 should fit file size");

        let mut rolling_bit_masks = [0; 8];
        // roll in a single 1
        rolling_bit_masks[0] = update_one_bit(0, true);
        for i in 1..8 {
            rolling_bit_masks[i] = update_one_bit(rolling_bit_masks[i - 1], false);
        }

        Self {
            buf,
            first: true,
            current_crc: add_zeroes(INITIAL_CRC, buf_size_u64) ^ FINAL_CRC_XOR,
            rolling_bit_masks,
        }
    }

    /// First return the crc32 of `buf`, then progressively replace bytes at the end with zero.
    /// Intermediate results will be the crc32 of `buf` filled with zeroes from the end. The final
    /// result will be the crc32 of all zeroes.
    pub fn new_zero_from_end(buf: &'a [u8]) -> Self {
        Self {
            current_crc: crc32fast::hash(buf),
            ..Self::new_common(buf)
        }
    }

    /// First return the crc32 of all zeroes, then progressively fill in the bytes from `buf`,
    /// starting from the end. Intermediate results will be the crc32 of a `buf` filled with zeroes
    /// from the start. The final result will be the crc32 of `buf`.
    pub fn new_fill_from_end(buf: &'a [u8]) -> Self {
        Self::new_common(buf)
    }
}

impl Iterator for PartialHasher<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.first {
            self.first = false;
            return Some(self.current_crc);
        }

        let (last_byte, rest) = self.buf.split_last()?;

        for i in 0..8 {
            let bit_mask = self.rolling_bit_masks[i];
            if last_byte & (0x80 >> i) != 0 {
                self.current_crc ^= bit_mask;
            }
            self.rolling_bit_masks[i] = BYTE_TABLE[usize::from(bit_mask as u8)] ^ (bit_mask >> 8);
        }
        self.buf = rest;

        Some(self.current_crc)
    }
}

const INITIAL_CRC: u32 = !0;
const FINAL_CRC_XOR: u32 = !0;
const IEEE_802_3_POLY: u32 = 0xEDB88320; // lsb-first

const fn update_one_bit(mut crc: u32, b: bool) -> u32 {
    if b {
        crc ^= 1;
    }

    let must_subtract = crc & 1 != 0;
    crc >>= 1;
    if must_subtract {
        crc ^ IEEE_802_3_POLY
    } else {
        crc
    }
}

const fn mult_mod(v0: u32, v1: u32) -> u32 {
    // Note: bit 0 of p is unused
    let mut p: u64 = 0;
    let mut i = 0;
    while i <= 31 {
        if v0 & (1u32 << i) != 0 {
            p ^= (v1 as u64) << (i + 1);
        }
        i += 1;
    }

    let mut i = 1;
    while i <= 31 {
        if p & (1u64 << i) != 0 {
            p ^= (IEEE_802_3_POLY as u64) << (i + 1);
        }
        i += 1;
    }
    (p >> 32) as u32
}

const fn compute_byte_powers() -> [u32; 64] {
    let mut powers = [0; 64];

    // Start with x^8, one followed by 1 byte of zeroes
    powers[0] = 0x00_80_00_00;
    let mut i = 1;
    while i < 64 {
        powers[i] = mult_mod(powers[i - 1], powers[i - 1]);
        i += 1;
    }

    powers
}

// `BYTE_POWERS[k] = (x ^ (8 * 2 ^ k)) MOD IEEE_802_3_POLY`
const BYTE_POWERS: [u32; 64] = compute_byte_powers();

const fn add_zeroes(mut crc: u32, mut block_size: u64) -> u32 {
    let mut power_i = 0;
    while block_size != 0 {
        if block_size & 1 != 0 {
            crc = mult_mod(crc, BYTE_POWERS[power_i]);
        }
        block_size >>= 1;
        power_i += 1;
    }

    crc
}

const fn compute_byte_table() -> [u32; 256] {
    let mut i = 0;
    let mut table = [0; 256];

    while i < 256 {
        let mut crc = 0;

        let mut j = 0;
        while j < 8 {
            crc = update_one_bit(crc, (i >> j) & 1 != 0);
            j += 1;
        }

        table[i] = crc;
        i += 1;
    }

    table
}

static BYTE_TABLE: [u32; 256] = compute_byte_table();

#[cfg(test)]
mod test {
    use super::FINAL_CRC_XOR;
    use super::INITIAL_CRC;
    use super::PartialHasher;
    use rand_xoshiro::Xoshiro256StarStar;
    use rand_xoshiro::rand_core::RngCore;
    use rand_xoshiro::rand_core::SeedableRng;

    #[test]
    fn test_add_zeroes() {
        for i in 0u16..=1024 {
            let add_zeroes_crc = super::add_zeroes(INITIAL_CRC, u64::from(i)) ^ FINAL_CRC_XOR;

            assert_eq!(add_zeroes_crc, crc32fast::hash(&vec![0; usize::from(i)]));
        }

        let mut rand = Xoshiro256StarStar::seed_from_u64(0);
        let mut target = vec![0; 1024];
        rand.fill_bytes(target.as_mut_slice());

        for i in 0..=target.len() {
            let truncated: Vec<u8> = target
                .iter()
                .enumerate()
                .map(|(ci, c)| if ci < i { *c } else { 0 })
                .collect();

            let add_zeroes_crc = super::add_zeroes(
                crc32fast::hash(&target[0..i]) ^ FINAL_CRC_XOR,
                u64::try_from(target.len() - i).unwrap(),
            ) ^ FINAL_CRC_XOR;
            assert_eq!(add_zeroes_crc, crc32fast::hash(&truncated));
        }
    }

    fn true_initial_truncate_crc(target: &[u8], first_non_truncated_byte: usize) -> u32 {
        let mut expected_crc_hasher = crc32fast::Hasher::new();
        expected_crc_hasher.update(&vec![0; first_non_truncated_byte]);
        expected_crc_hasher.update(&target[first_non_truncated_byte..]);

        expected_crc_hasher.finalize()
    }

    #[test]
    fn test_init_truncate_small() {
        let mut rand = Xoshiro256StarStar::seed_from_u64(1);
        for i in 0..258 {
            let mut target = vec![0; i];
            rand.fill_bytes(target.as_mut_slice());

            let target = target.as_slice();
            let mut rolling = PartialHasher::new_fill_from_end(target);

            for first_non_truncated_byte in (0..=target.len()).rev() {
                let expected_crc = true_initial_truncate_crc(target, first_non_truncated_byte);
                let actual_crc = rolling.next().unwrap();
                assert_eq!(
                    expected_crc, actual_crc,
                    "expected_crc {expected_crc:#010x} != actual_crc {actual_crc:#010x} (byte {first_non_truncated_byte})"
                );
            }

            assert!(rolling.next().is_none());
        }
    }

    #[ignore]
    #[test]
    fn test_init_truncate_large() {
        // spot check random points in a few large files
        let mut rand = Xoshiro256StarStar::seed_from_u64(2);
        for size in [1 * 1024 * 1024, 1 * 1000 * 1000] {
            let mut target = vec![0; size];
            rand.fill_bytes(target.as_mut_slice());

            let target = target.as_slice();

            for tries in 0..100 {
                let first_non_truncated_byte = match tries {
                    0 => 0,
                    1 => size,
                    _ => usize::try_from(rand.next_u64()).unwrap() % (size + 1),
                };

                let expected_crc = true_initial_truncate_crc(target, first_non_truncated_byte);

                let mut rolling = PartialHasher::new_fill_from_end(target);

                for i in (0..=size).rev() {
                    let actual_crc = rolling.next().unwrap();
                    if i == first_non_truncated_byte {
                        assert_eq!(
                            expected_crc, actual_crc,
                            "expected_crc {expected_crc:#010x} != actual_crc {actual_crc:#010x} (byte {first_non_truncated_byte})"
                        );
                    }
                }
                assert!(rolling.next().is_none());
            }
        }
    }

    fn true_truncate_crc(target: &[u8], first_truncated_byte: usize) -> u32 {
        assert!(first_truncated_byte <= target.len());
        let mut expected_crc_hasher = crc32fast::Hasher::new();
        expected_crc_hasher.update(&target[..first_truncated_byte]);
        expected_crc_hasher.update(&vec![0; target.len() - first_truncated_byte]);

        expected_crc_hasher.finalize()
    }

    #[test]
    fn test_truncate_small() {
        let mut rand = Xoshiro256StarStar::seed_from_u64(3);
        for i in 0..258 {
            let mut target = vec![0; i];
            rand.fill_bytes(target.as_mut_slice());

            let target = target.as_slice();
            let mut rolling = PartialHasher::new_zero_from_end(target);

            for first_truncated_byte in (0..=target.len()).rev() {
                let expected_crc = true_truncate_crc(target, first_truncated_byte);
                let actual_crc = rolling.next().unwrap();
                assert_eq!(
                    expected_crc, actual_crc,
                    "expected_crc {expected_crc:#010x} != actual_crc {actual_crc:#010x} (byte {first_truncated_byte})"
                );
            }

            assert!(rolling.next().is_none());
        }
    }

    #[ignore]
    #[test]
    fn test_truncate_large() {
        // spot check random points in a few large files
        let mut rand = Xoshiro256StarStar::seed_from_u64(4);
        for size in [1 * 1024 * 1024, 1 * 1000 * 1000] {
            let mut target = vec![0; size];
            rand.fill_bytes(target.as_mut_slice());

            let target = target.as_slice();

            for tries in 0..100 {
                let first_truncated_byte = match tries {
                    0 => 0,
                    1 => size,
                    _ => usize::try_from(rand.next_u64()).unwrap() % (size + 1),
                };

                let expected_crc = true_truncate_crc(target, first_truncated_byte);

                let mut rolling = PartialHasher::new_zero_from_end(target);

                for i in (0..=size).rev() {
                    let actual_crc = rolling.next().unwrap();
                    if i == first_truncated_byte {
                        assert_eq!(
                            expected_crc, actual_crc,
                            "expected_crc {expected_crc:#010x} != actual_crc {actual_crc:#010x} (byte {first_truncated_byte})"
                        );
                    }
                }
                assert!(rolling.next().is_none());
            }
        }
    }
}
