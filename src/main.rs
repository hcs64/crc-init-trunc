use std::io::Read;
use crc32fast::Hasher;

fn main() {
    let mut args = std::env::args_os();
    args.next().unwrap();
    let infile_name = args.next().expect("infile name");
    let target_crc = args.next().and_then(|oss| oss.into_string().ok()).and_then(|s| u32::from_str_radix(&s, 16).ok()).expect("target crc");
    let infile = std::fs::read(infile_name).expect("read infile");

    let source_crc = crc32fast::hash(&infile);
    let infile_size_u64 = u64::try_from(infile.len()).expect("u64 should hold usize");
    let all_zero_hash = zeroes_crc32(infile_size_u64);
    let mut current_crc = source_crc;
    let mut current_end_crc = all_zero_hash;;
    let mut pre_zeroes_hasher = Hasher::new();

    let mut zeroes_hasher = Hasher::new();
    let mut post_zeroes_crc = all_zero_hash;
    let zero_chunk_hasher = Hasher::new_with_initial_len(crc32fast::hash(&[0,0,0,0]), 4);

    let extend_hasher = Hasher::new_with_initial_len(zeroes_crc32(infile_size_u64-1), infile_size_u64-1);
    let mut rolling_mask: [u32; 8] = std::array::from_fn(|i| {
        let mut hasher = extend_hasher.clone();
        hasher.update(&[1 << i]);
        hasher.finalize() ^ INIT_CRC
    });
    let advance = block_advance_xor(infile_size_u64);

    //for data_start_samples in 0..=infile.len()/4 {
    for data_start_samples in (0..=infile.len()/4).rev() {
        let data_start = data_start_samples * 4;

        /*
        if data_start_samples % 1000*1000 == 0 {
            eprintln!("{:.2}%", (data_start as f64 / infile.len() as f64) * 100.);
        }
        */


        if false {
            let mut hasher = zeroes_hasher.clone();
            hasher.update(&infile[data_start..]);
            let crc = hasher.finalize();

            //assert_eq!(current_crc, crc, "data_start={data_start}, current_crc={current_crc:#x}, crc={crc:#x}");
            assert_eq!(current_end_crc, crc, "data_start={data_start}, current_crc={current_crc:#x}, crc={crc:#x}");

            if crc == target_crc {
                println!("start: {data_start:#x}");
            }
            zeroes_hasher.update(&[0,0,0,0]);
        }
        if true {
            if current_end_crc == target_crc {
                println!("start: {data_start:#x}");
            }

            if data_start < 4 {
                break;
            }
            let next_data_start = data_start - 4;

            for j in 0..4 {
                for i in 0..8 {
                    if infile[next_data_start + 3 - j] & (1 << i) != 0 {
                        current_end_crc ^= (rolling_mask[i] ^ INIT_CRC) ^ all_zero_hash;
                    }
                }
                for i in 0..8 {
                    rolling_mask[i] = update_0(rolling_mask[i]) ^ advance;
                }
            }
        }
        if false {
            if current_crc == target_crc {
                println!("start: {data_start:#x}");
            }

            let post_zeroes_start = data_start + 4;
            if post_zeroes_start >= infile.len() {
                break;
            }
            let post_len = u64::try_from(infile.len() - post_zeroes_start).expect("u64 should hold usize");

            // trim prefix 4 from post zeroes
            let mut post_zeroes_hasher = zero_chunk_hasher.clone();
            post_zeroes_hasher.combine(
                &crc32fast::Hasher::new_with_initial_len(
                    post_zeroes_crc,
                    post_len,
                ));
            post_zeroes_crc = post_zeroes_hasher.finalize();

            let mut mask_hasher = pre_zeroes_hasher.clone();
            mask_hasher.update(&infile[data_start..post_zeroes_start]);
            mask_hasher.combine(&crc32fast::Hasher::new_with_initial_len(post_zeroes_crc, post_len));
            current_crc ^= mask_hasher.finalize() ^ all_zero_hash;

            //pre_zeroes_hasher.combine(&zero_chunk_hasher);
            pre_zeroes_hasher.update(&[0, 0, 0, 0]);
        }
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

    let last = acc.finalize() ^ INIT_CRC;
    last ^ update_0(last)
}


#[cfg(test)]
mod test {
    #[test]
    fn test_truncate() {
        // removing bytes from the start
    }

    #[test]
    fn test_init_truncate() {
        // removing bytes from the end
    }
}
