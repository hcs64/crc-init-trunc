use crc32fast::Hasher;

pub struct PartialHasher<'a> {
    buf: &'a [u8],
    first: bool,

    all_zero: u32,
    current_crc: u32,
    rolling_bit_mask: u32,
    advance_one_bit: u32,
}

impl<'a> PartialHasher<'a> {
    fn new_common(buf: &'a [u8]) -> Self {
        if buf.is_empty() {
            let crc = crc32fast::hash(b"");
            return Self {
                buf,
                first: true,
                all_zero: crc ^ FINAL_CRC_XOR,
                current_crc: crc,
                rolling_bit_mask: 0,
                advance_one_bit: 0,
            };
        }

        let buf_size_u64 = u64::try_from(buf.len()).expect("u64 should fit file size");

        let all_zero_crc = zeroes_crc32(buf_size_u64);

        let advance_one_bit =
            all_zero_crc ^ FINAL_CRC_XOR ^ update_one_bit(all_zero_crc ^ FINAL_CRC_XOR, false);

        let rolling_bit_mask = {
            let mut hasher =
                Hasher::new_with_initial_len(zeroes_crc32(buf_size_u64 - 1), buf_size_u64 - 1);
            hasher.update(&[0x80]); // last bit (lsb-first)
            hasher.finalize() ^ FINAL_CRC_XOR
        };

        Self {
            buf,
            first: true,
            all_zero: all_zero_crc ^ FINAL_CRC_XOR,
            current_crc: all_zero_crc,
            rolling_bit_mask,
            advance_one_bit,
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

        for i in (0..8).rev() {
            if last_byte & (1 << i) != 0 {
                self.current_crc ^= self.rolling_bit_mask ^ self.all_zero;
            }
            self.rolling_bit_mask =
                update_one_bit(self.rolling_bit_mask, false) ^ self.advance_one_bit;
        }
        self.buf = rest;

        Some(self.current_crc)
    }
}

fn zeroes_crc32(block_size: u64) -> u32 {
    // Get the CRC of a block of zeroes by adding powers of 2
    let mut pow2_zero_block_crc = crc32fast::hash(&[0]);
    let mut acc = Hasher::new();
    for n in 0..=63 {
        let pow2 = 1u64 << n;
        let mut h = Hasher::new_with_initial_len(pow2_zero_block_crc, pow2);
        if (block_size & pow2) != 0 {
            acc.combine(&h);
        } else if block_size < pow2 {
            break;
        }
        h.combine(&h.clone());
        pow2_zero_block_crc = h.finalize();
    }

    acc.finalize()
}

const FINAL_CRC_XOR: u32 = !0;

fn update_one_bit(mut crc: u32, b: bool) -> u32 {
    const IEEE_802_3_POLY: u32 = 0xEDB88320; // lsb-first

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

#[cfg(test)]
mod test {
    use super::PartialHasher;
    use rand_xoshiro::Xoshiro256StarStar;
    use rand_xoshiro::rand_core::RngCore;
    use rand_xoshiro::rand_core::SeedableRng;

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
