//! AES-128-ECB + PKCS7 密码加密，输出 Base64。
//!
//! 1:1 复刻 Python `xd_xk/encrypt.py`：密钥固定为 `MWMqg2tPcDkxcm11`
//! （16 字节 = AES-128），ECB 模式无 IV，PKCS7 填充，Base64 输出。

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes128;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

/// 密码加密密钥（与 Python 版一致，勿改）。
pub const AES_KEY: &str = "MWMqg2tPcDkxcm11";

/// AES 块大小（字节）。
const BLOCK_SIZE: usize = 16;

/// PKCS7 填充：补足到块大小的整数倍，填充字节值 = 补足长度。
fn pkcs7_pad(data: &[u8], block_size: usize) -> Vec<u8> {
    let pad_len = block_size - (data.len() % block_size);
    let mut out = Vec::with_capacity(data.len() + pad_len);
    out.extend_from_slice(data);
    out.extend(std::iter::repeat(pad_len as u8).take(pad_len));
    out
}

/// AES-128-ECB 加密，返回 Base64 字符串。
///
/// `key` 需为 16 字节；`plaintext` 为 UTF-8 明文。
pub fn aes_encrypt(key: &str, plaintext: &str) -> String {
    let cipher = Aes128::new_from_slice(key.as_bytes()).expect("AES-128 密钥需 16 字节");
    let padded = pkcs7_pad(plaintext.as_bytes(), BLOCK_SIZE);

    let mut out = Vec::with_capacity(padded.len());
    for chunk in padded.chunks(BLOCK_SIZE) {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        out.extend_from_slice(&block);
    }
    BASE64.encode(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试向量由 Python 版 `AES_encrypt` 生成（.venv 中执行），作为权威基准。
    #[test]
    fn matches_python_vectors() {
        assert_eq!(aes_encrypt(AES_KEY, "123456"), "OSfRhnd673K1Lp6cP4L6nA==");
        assert_eq!(aes_encrypt(AES_KEY, "hello"), "zTB/3Oiwdhio9uX5c1PYEA==");
    }

    #[test]
    fn pkcs7_padding_is_correct() {
        // 15 字节 → 补 1 字节 0x01
        assert_eq!(pkcs7_pad(b"a".repeat(15).as_slice(), 16), {
            let mut v = b"a".repeat(15);
            v.push(1);
            v
        });
        // 16 字节（整块）→ 追加一整块 0x10
        assert_eq!(pkcs7_pad(b"b".repeat(16).as_slice(), 16), {
            let mut v = b"b".repeat(16);
            v.extend(std::iter::repeat(16u8).take(16));
            v
        });
    }
}
