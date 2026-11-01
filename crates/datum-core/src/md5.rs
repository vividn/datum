//! MD5 (RFC 1321), written here because all datum asks of it is a stable
//! number from a name: v2's colors and lane positions. Nothing here is a
//! security property.

/// `floor(2^32 * |sin(i + 1)|)`, RFC 1321's table.
const K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

/// Each round's four left rotations.
const S: [[u32; 4]; 4] = [
    [7, 12, 17, 22],
    [5, 9, 14, 20],
    [4, 11, 16, 23],
    [6, 10, 15, 21],
];

pub fn digest(input: &[u8]) -> [u8; 16] {
    let mut state = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let mut blocks = input.chunks_exact(64);
    for block in &mut blocks {
        compress(&mut state, block);
    }
    // What is left, a 1 bit, and the length in bits in the last 8 bytes of a
    // block: a second block where they do not fit in this one.
    let rest = blocks.remainder();
    let mut tail = [0u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let end = if rest.len() < 56 { 64 } else { 128 };
    let bits = (input.len() as u64).wrapping_mul(8);
    tail[end - 8..end].copy_from_slice(&bits.to_le_bytes());
    for block in tail[..end].chunks_exact(64) {
        compress(&mut state, block);
    }

    let mut out = [0u8; 16];
    for (bytes, word) in out.chunks_exact_mut(4).zip(state) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    out
}

fn compress(state: &mut [u32; 4], block: &[u8]) {
    let word = |i: usize| {
        u32::from_le_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ])
    };
    let [mut a, mut b, mut c, mut d] = *state;
    for i in 0..64 {
        let (f, g) = match i / 16 {
            0 => ((b & c) | (!b & d), i),
            1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let sum = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(word(g));
        (a, d, c) = (d, c, b);
        b = b.wrapping_add(sum.rotate_left(S[i / 16][i % 4]));
    }
    for (held, new) in state.iter_mut().zip([a, b, c, d]) {
        *held = held.wrapping_add(new);
    }
}

#[cfg(test)]
mod tests {
    use super::digest;

    fn hex(input: &[u8]) -> String {
        digest(input).iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn rfc_1321_test_suite() {
        for (input, want) in [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "d174ab98d277d9f5a5611c2c9f419d9f",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ] {
            assert_eq!(hex(input.as_bytes()), want, "md5({input:?})");
        }
    }

    /// RFC 1321's messages cross none of the lengths the padding turns on.
    /// These digests are coreutils' `md5sum`'s, of that many `a`s.
    #[test]
    fn the_lengths_the_padding_turns_on() {
        for (len, want) in [
            (55, "ef1772b6dff9a122358552954ad0df65"),
            (56, "3b0c8ac703f828b04c6c197006d17218"),
            (57, "652b906d60af96844ebd21b674f35e93"),
            (63, "b06521f39153d618550606be297466d5"),
            (64, "014842d480b571495a4a0363793f7367"),
            (65, "c743a45e0d2e6a95cb859adae0248435"),
            (119, "8a7bd0732ed6a28ce75f6dabc90e1613"),
            (120, "5f61c0ccad4cac44c75ff505e1f1e537"),
            (1000, "cabe45dcc9ae5b66ba86600cca6b8ba8"),
        ] {
            assert_eq!(hex(&vec![b'a'; len]), want, "md5 of {len} a's");
        }
    }
}
