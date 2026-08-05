//! Minimal SHA-1 implementation used to compute git blob hashes for the
//! `index` line of unified diffs.
//!
//! A git blob hash is `sha1("blob <length>\0" + content)`.

/// Computes the SHA-1 digest of `data` and returns it as lowercase hex.
pub fn sha1_hex(data: &[u8]) -> String {
    let mut h0: u32 = 0x6745_2301;
    let mut h1: u32 = 0xefcd_ab89;
    let mut h2: u32 = 0x98ba_dcfe;
    let mut h3: u32 = 0x1032_5476;
    let mut h4: u32 = 0xc3d2_e1f0;

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    format!("{h0:08x}{h1:08x}{h2:08x}{h3:08x}{h4:08x}")
}

/// Computes the git blob hash for `content`.
///
/// Equivalent to `git hash-object` on the given bytes.
pub fn git_blob_hash(content: &[u8]) -> String {
    let mut buf = Vec::with_capacity(content.len() + 32);
    buf.extend_from_slice(format!("blob {}\0", content.len()).as_bytes());
    buf.extend_from_slice(content);
    sha1_hex(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_known_vectors() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            sha1_hex(b"The quick brown fox jumps over the lazy dog"),
            "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12"
        );
    }

    #[test]
    fn test_git_blob_hash_matches_git_hash_object() {
        assert_eq!(
            git_blob_hash(b""),
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        );
        assert_eq!(
            git_blob_hash(b"hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }
}
