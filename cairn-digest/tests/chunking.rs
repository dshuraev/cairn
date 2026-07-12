use cairn_core::hash::HashAlgorithm;
use cairn_digest::chunk::{chunk_file, ChunkConfig};
use std::io::Write;
use std::path::PathBuf;

/// Deterministic pseudo-random bytes (xorshift64), so tests don't need a `rand`
/// dependency but still get enough variation to cross several chunk boundaries.
fn deterministic_bytes(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut buf = Vec::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        buf.push((state & 0xff) as u8);
    }
    buf
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cairn-digest-test-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn chunking_is_deterministic_and_covers_the_whole_file() {
    let dir = unique_temp_dir("chunking");
    let path = dir.join("data.bin");
    let content = deterministic_bytes(300 * 1024);
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&content).unwrap();
    drop(file);

    let config = ChunkConfig::default();

    let first = chunk_file(&path, &config, HashAlgorithm::Sha256).unwrap();
    let second = chunk_file(&path, &config, HashAlgorithm::Sha256).unwrap();

    let first_ids: Vec<_> = first.iter().map(|(id, _)| *id).collect();
    let second_ids: Vec<_> = second.iter().map(|(id, _)| *id).collect();
    assert_eq!(first_ids, second_ids, "chunking must be deterministic");

    let total_len: usize = first.iter().map(|(_, data)| data.len()).sum();
    assert_eq!(total_len, content.len());

    let n = first.len();
    assert!(
        n > 1,
        "300KB with 64KiB avg chunk size should produce multiple chunks"
    );
    for (i, (_, data)) in first.iter().enumerate() {
        if i + 1 < n {
            assert!(
                data.len() >= config.min_size && data.len() <= config.max_size,
                "non-final chunk {i} has length {} outside [{}, {}]",
                data.len(),
                config.min_size,
                config.max_size
            );
        }
    }

    std::fs::remove_dir_all(&dir).unwrap();
}
