crc-init-trunc
==============


Search for a truncation point to match the given crc32

Using some tricks it runs an exhaustive search pretty quickly, under 10 seconds for a 700MB file.

Note that if there are multiple matches they will be printed to standard out on multiple lines.

## Usage

```
crc-init-trunc infile.bin target_crc [--truncate-start|--truncate-end]
```

---

Find a match where a file is zeroed from the start (default mode):

```
crc-init-trunc infile.bin abcd1234
```
or
```
crc-init-trunc infile.bin abcd1234 --truncate-start
```

Example output:

```
matches with 0 from start until 0x227
```

---

Find a match where a file is zeroed at the end (crctrunc mode):

```
crc-init-trunc infile.bin abcd1234 --truncate-end
```

Example output:

```
matches with 0 from 0x2c10857f until end
```
