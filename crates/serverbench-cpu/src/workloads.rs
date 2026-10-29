use serde::{Deserialize, Serialize};
use serverbench_core::{Error, Result};
use sha2::{Digest, Sha256};
use std::hint::black_box;

pub const DATASET_BYTES: usize = 65536;
pub const INTEGER_UNITS: u64 = 32768;
pub const DATA_OBJECTS: u64 = 256;
pub use serverbench_core::DATASET_SHA256;
pub const INTEGER_CHECKSUM: u64 = 4644254777524890854;
pub const DATA_CHECKSUM: u64 = 4610356586914610349;

pub fn dataset() -> Vec<u8> {
    let phrase = b"ServerBench deterministic CPU corpus. records: alpha beta gamma 0123456789\n";
    (0..DATASET_BYTES)
        .map(|i| {
            if i % 97 < 80 {
                phrase[i % phrase.len()]
            } else {
                ((i * 31 + i / 97) % 256) as u8
            }
        })
        .collect()
}
pub fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}
pub fn integer() -> u64 {
    let mut x = black_box(0x123456789abcdef0u64);
    let table: [u64; 256] = std::array::from_fn(|i| (i as u64).wrapping_mul(0x9e3779b97f4a7c15));
    let mut sum = 0u64;
    for i in 0..black_box(INTEGER_UNITS) {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(i ^ 1442695040888963407);
        x = if x & 8 == 0 {
            x.rotate_left(13) ^ table[(x & 255) as usize]
        } else {
            x.rotate_right(7)
                .wrapping_add(table[((x >> 8) & 255) as usize])
        };
        sum = sum.wrapping_add(x ^ (x >> 23));
    }
    black_box(sum)
}
#[derive(Serialize, Deserialize)]
struct Record {
    id: u32,
    name: String,
    value: i64,
    active: bool,
}
pub fn records() -> Vec<u8> {
    let rows: Vec<_> = (0..DATA_OBJECTS as u32)
        .map(|id| Record {
            id,
            name: format!("record-{id:04}-alpha"),
            value: ((id as i64 * 7919) % 100003) - 50000,
            active: id % 3 != 0,
        })
        .collect();
    serde_json::to_vec(&rows).expect("serializable records")
}
pub fn process(records: &[u8]) -> Result<u64> {
    let rows: Vec<Record> = serde_json::from_slice(black_box(records))?;
    let mut rows: Vec<_> = rows
        .into_iter()
        .filter(|r| r.active)
        .map(|mut r| {
            r.value = r.value * 3 + 17;
            r.name = r.name.replace("alpha", "BETA").to_uppercase();
            r
        })
        .collect();
    rows.sort_unstable_by_key(|r| (r.value, r.id));
    let index: std::collections::BTreeMap<_, _> = rows
        .iter()
        .map(|r| (r.id, fnv(r.name.as_bytes())))
        .collect();
    let lookup = rows
        .iter()
        .fold(0u64, |h, r| h.wrapping_add(*index.get(&r.id).unwrap()));
    Ok(fnv(&serde_json::to_vec(&rows)?) ^ lookup)
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Integer,
    Compression,
    Decompression,
    Hash,
    Data,
    Mixed,
}
impl Kind {
    pub const ALL: [Self; 5] = [
        Self::Integer,
        Self::Compression,
        Self::Decompression,
        Self::Hash,
        Self::Data,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Compression => "compression",
            Self::Decompression => "decompression",
            Self::Hash => "hash",
            Self::Data => "data",
            Self::Mixed => "mixed",
        }
    }
    pub fn unit(self) -> &'static str {
        match self {
            Self::Integer => "integer_iterations/s",
            Self::Data => "objects/s",
            Self::Mixed => "mixed_cycles/s",
            _ => "bytes/s",
        }
    }
    pub fn units(self) -> u64 {
        match self {
            Self::Integer => INTEGER_UNITS,
            Self::Data => DATA_OBJECTS,
            Self::Mixed => 1,
            _ => DATASET_BYTES as u64,
        }
    }
}
pub struct Workload {
    data: Vec<u8>,
    compressed: Vec<u8>,
    records: Vec<u8>,
    digest: [u8; 32],
}
impl Workload {
    pub fn new() -> Result<Self> {
        let data = dataset();
        let digest: [u8; 32] = Sha256::digest(&data).into();
        if format!("{:x}", Sha256::digest(&data)) != DATASET_SHA256 {
            return Err(Error::new(4, "benchmark dataset checksum mismatch"));
        }
        let compressed = miniz_oxide::deflate::compress_to_vec(&data, 6);
        if miniz_oxide::inflate::decompress_to_vec(&compressed)
            .ok()
            .as_ref()
            != Some(&data)
        {
            return Err(Error::new(4, "compression round-trip failed"));
        }
        Ok(Self {
            data,
            compressed,
            records: records(),
            digest,
        })
    }
    pub fn execute(&mut self, kind: Kind) -> Result<u64> {
        let valid = match kind {
            Kind::Integer => integer() == INTEGER_CHECKSUM,
            Kind::Compression => {
                miniz_oxide::deflate::compress_to_vec(black_box(&self.data), 6) == self.compressed
            }
            Kind::Decompression => {
                miniz_oxide::inflate::decompress_to_vec(black_box(&self.compressed))
                    .ok()
                    .as_ref()
                    == Some(&self.data)
            }
            Kind::Hash => {
                let digest: [u8; 32] = Sha256::digest(black_box(&self.data)).into();
                black_box(digest) == self.digest
            }
            Kind::Data => process(&self.records)? == DATA_CHECKSUM,
            Kind::Mixed => {
                for k in Kind::ALL {
                    black_box(self.execute(k)?);
                }
                true
            }
        };
        if !black_box(valid) {
            return Err(Error::new(
                4,
                format!("benchmark invalid: {} checksum mismatch", kind.name()),
            ));
        }
        Ok(black_box(kind.units()))
    }
    pub fn validate() -> Result<()> {
        let mut w = Self::new()?;
        for k in Kind::ALL {
            w.execute(k)?;
        }
        // Independent SHA-256 known-answer test.
        if format!("{:x}", Sha256::digest(b"abc"))
            != "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        {
            return Err(Error::new(4, "SHA-256 known-answer test failed"));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_known_answers() {
        Workload::validate().unwrap();
        assert_eq!(integer(), INTEGER_CHECKSUM);
        assert_eq!(process(&records()).unwrap(), DATA_CHECKSUM);
    }
    #[test]
    fn checksum_corruption_rejected() {
        let mut w = Workload::new().unwrap();
        w.data[0] ^= 1;
        assert!(w.execute(Kind::Hash).is_err());
        assert!(w.execute(Kind::Compression).is_err());
    }
}
