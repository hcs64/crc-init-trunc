mod partial_hasher;

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if !(args.len() == 3 || args.len() == 4) {
        eprintln!("crc-init-trunc");
        eprintln!("  Search for a truncation point to match the given crc32");
        eprintln!();

        eprintln!("Usage: crc-init-trunc infile.bin target_crc [--truncate-start|--truncate-end]");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  Find a match where a file is zeroed from the start (default mode):");
        eprintln!("    crc-init-trunc infile.bin abcd1234");
        eprintln!("      or");
        eprintln!("    crc-init-trunc infile.bin abcd1234 --truncate-start");
        eprintln!();
        eprintln!("  Find a match where a file is zeroed at the end (crctrunc mode):");
        eprintln!("    crc-init-trunc infile.bin abcd1234 --truncate-end");
        return;
    }

    let infile_name = &args[1];
    let target_crc = args[2]
        .to_str()
        .and_then(|s| u32::from_str_radix(s, 16).ok())
        .expect("should have valid hexadecimal target crc32");

    enum Mode {
        Start,
        End,
    }
    let mode = match args
        .get(3)
        .map(|s| s.to_str().expect("should have valid string for mode"))
    {
        // default mode
        None => Mode::Start,
        Some("--truncate-start") => Mode::Start,
        Some("--truncate-end") => Mode::End,
        Some(s) => {
            eprintln!("invalid mode {s:?}");
            return;
        }
    };

    let infile = std::fs::read(infile_name).expect("read infile");

    match mode {
        Mode::Start => {
            let mut hasher = partial_hasher::PartialHasherFillFromEnd::new(&infile);

            for first_non_truncated_byte in (0..=infile.len()).rev() {
                let current_crc = hasher.next().unwrap();
                if current_crc == target_crc {
                    println!("matches with 0 from start until {first_non_truncated_byte:#x}");
                }
            }
        }
        Mode::End => {
            let mut hasher = partial_hasher::PartialHasherZeroFromEnd::new(&infile);

            for first_truncated_byte in (0..=infile.len()).rev() {
                let current_crc = hasher.next().unwrap();
                if current_crc == target_crc {
                    println!("matches with 0 from {first_truncated_byte:#x} until end");
                }
            }
        }
    }
}
