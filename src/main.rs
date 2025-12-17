use std::io::Read;
use crc32fast::Hasher;

fn main() {
    let mut args = std::env::args_os();
    args.next().unwrap();
    let infile_name = args.next().expect("infile name");
    let target_crc = args.next().and_then(|oss| oss.into_string().ok()).and_then(|s| u32::from_str_radix(&s, 16).ok()).expect("target crc");
    let infile = std::fs::read(infile_name).expect("read infile");

    let mut zeroes_hasher = Hasher::new();
    for data_start_samples in 0..=infile.len()/4 {
        let data_start = data_start_samples * 4;
        let mut hasher = zeroes_hasher.clone();
        hasher.update(&infile[data_start..]);
        let crc = hasher.finalize();
        if crc == target_crc {
            println!("start: {data_start:#x}");
        }

        zeroes_hasher.update(&[0,0,0,0]);
    }
}
