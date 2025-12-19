use crc32fast::Hasher;

pub struct PartialHasher<'a> {
    buf: &'a [u8],
    first: bool,

    all_zero: u32,
    current_crc: u32,
    rolling_mask: [u32; 8],
    advance_xor: u32,
}

impl<'a> PartialHasher<'a> {
    fn new_common(buf: &'a [u8]) -> Self {
        if buf.is_empty() {
            let crc = crc32fast::hash(b"");
            return Self {
                buf,
                first: true,
                all_zero: crc ^ INIT_CRC,
                current_crc: crc,
                rolling_mask: [0; 8],
                advance_xor: 0,
            };
        }

        let buf_size_u64 = u64::try_from(buf.len()).expect("u64 should fit file size");

        let all_zero_crc = zeroes_crc32(buf_size_u64);

        let extend_hasher =
            Hasher::new_with_initial_len(zeroes_crc32(buf_size_u64 - 1), buf_size_u64 - 1);
        let rolling_mask = std::array::from_fn(|i| {
            let mut hasher = Hasher::new();
            hasher.combine(&extend_hasher);
            hasher.update(&[1 << i]);
            hasher.finalize() ^ INIT_CRC
        });
        let advance_xor = block_advance_xor(buf_size_u64);

        Self {
            buf,
            first: true,
            all_zero: all_zero_crc ^ INIT_CRC,
            current_crc: all_zero_crc,
            rolling_mask,
            advance_xor,
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

        // update the crc, setting the last byte to or from 0
        for i in 0..8 {
            if last_byte & (1 << i) != 0 {
                let mask_crc = self.rolling_mask[i];
                let diff = mask_crc ^ self.all_zero;
                self.current_crc ^= diff;
            }
        }

        // roll the bit masks to apply to the previous byte
        self.buf = rest;
        if !self.buf.is_empty() {
            for i in 0..8 {
                self.rolling_mask[i] = update_0(self.rolling_mask[i]) ^ self.advance_xor;
            }
        }

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

const INIT_CRC: u32 = !0;
const IEEE_TABLE: [u32; 256] = [
    0, 1996959894, 3993919788, 2567524794, 124634137, 1886057615, 3915621685, 2657392035,
    249268274, 2044508324, 3772115230, 2547177864, 162941995, 2125561021, 3887607047, 2428444049,
    498536548, 1789927666, 4089016648, 2227061214, 450548861, 1843258603, 4107580753, 2211677639,
    325883990, 1684777152, 4251122042, 2321926636, 335633487, 1661365465, 4195302755, 2366115317,
    997073096, 1281953886, 3579855332, 2724688242, 1006888145, 1258607687, 3524101629, 2768942443,
    901097722, 1119000684, 3686517206, 2898065728, 853044451, 1172266101, 3705015759, 2882616665,
    651767980, 1373503546, 3369554304, 3218104598, 565507253, 1454621731, 3485111705, 3099436303,
    671266974, 1594198024, 3322730930, 2970347812, 795835527, 1483230225, 3244367275, 3060149565,
    1994146192, 31158534, 2563907772, 4023717930, 1907459465, 112637215, 2680153253, 3904427059,
    2013776290, 251722036, 2517215374, 3775830040, 2137656763, 141376813, 2439277719, 3865271297,
    1802195444, 476864866, 2238001368, 4066508878, 1812370925, 453092731, 2181625025, 4111451223,
    1706088902, 314042704, 2344532202, 4240017532, 1658658271, 366619977, 2362670323, 4224994405,
    1303535960, 984961486, 2747007092, 3569037538, 1256170817, 1037604311, 2765210733, 3554079995,
    1131014506, 879679996, 2909243462, 3663771856, 1141124467, 855842277, 2852801631, 3708648649,
    1342533948, 654459306, 3188396048, 3373015174, 1466479909, 544179635, 3110523913, 3462522015,
    1591671054, 702138776, 2966460450, 3352799412, 1504918807, 783551873, 3082640443, 3233442989,
    3988292384, 2596254646, 62317068, 1957810842, 3939845945, 2647816111, 81470997, 1943803523,
    3814918930, 2489596804, 225274430, 2053790376, 3826175755, 2466906013, 167816743, 2097651377,
    4027552580, 2265490386, 503444072, 1762050814, 4150417245, 2154129355, 426522225, 1852507879,
    4275313526, 2312317920, 282753626, 1742555852, 4189708143, 2394877945, 397917763, 1622183637,
    3604390888, 2714866558, 953729732, 1340076626, 3518719985, 2797360999, 1068828381, 1219638859,
    3624741850, 2936675148, 906185462, 1090812512, 3747672003, 2825379669, 829329135, 1181335161,
    3412177804, 3160834842, 628085408, 1382605366, 3423369109, 3138078467, 570562233, 1426400815,
    3317316542, 2998733608, 733239954, 1555261956, 3268935591, 3050360625, 752459403, 1541320221,
    2607071920, 3965973030, 1969922972, 40735498, 2617837225, 3943577151, 1913087877, 83908371,
    2512341634, 3803740692, 2075208622, 213261112, 2463272603, 3855990285, 2094854071, 198958881,
    2262029012, 4057260610, 1759359992, 534414190, 2176718541, 4139329115, 1873836001, 414664567,
    2282248934, 4279200368, 1711684554, 285281116, 2405801727, 4167216745, 1634467795, 376229701,
    2685067896, 3608007406, 1308918612, 956543938, 2808555105, 3495958263, 1231636301, 1047427035,
    2932959818, 3654703836, 1088359270, 936918000, 2847714899, 3736837829, 1202900863, 817233897,
    3183342108, 3401237130, 1404277552, 615818150, 3134207493, 3453421203, 1423857449, 601450431,
    3009837614, 3294710456, 1567103746, 711928724, 3020668471, 3272380065, 1510334235, 755167117,
];

fn update_0(crc: u32) -> u32 {
    IEEE_TABLE[(crc & 0xff) as usize] ^ (crc >> 8)
}

fn block_advance_xor(block_size: u64) -> u32 {
    let last = zeroes_crc32(block_size) ^ INIT_CRC;
    last ^ update_0(last)
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
