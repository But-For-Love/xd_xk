use aes::Aes128;
use aes::cipher::{Array, BlockCipherEncrypt, KeyInit};
use base64::Engine;

use crate::error::XkError;

/// 与 Python 版保持一致的 AES-128-ECB 密钥。
const AES_KEY: [u8; 16] = *b"MWMqg2tPcDkxcm11";
const BLOCK_SIZE: usize = 16;

/// 按西电选课系统要求加密密码：AES-128-ECB + PKCS#7 填充 + Base64。
pub fn aes_encrypt(plain: &str) -> Result<String, XkError> {
    let key = Array::from(AES_KEY);
    let cipher = Aes128::new(&key);

    let mut padded = plain.as_bytes().to_vec();
    let padding = BLOCK_SIZE - (padded.len() % BLOCK_SIZE);
    padded.extend(std::iter::repeat_n(padding as u8, padding));

    let mut encrypted = Vec::with_capacity(padded.len());
    for chunk in padded.chunks_exact(BLOCK_SIZE) {
        let fixed: [u8; BLOCK_SIZE] = chunk
            .try_into()
            .map_err(|_| XkError::Crypto("数据块长度异常".to_string()))?;
        let mut block = Array::from(fixed);
        cipher.encrypt_block(&mut block);
        encrypted.extend_from_slice(block.as_ref());
    }

    Ok(base64::engine::general_purpose::STANDARD.encode(encrypted))
}

#[cfg(test)]
mod tests {
    use super::aes_encrypt;

    #[test]
    fn encrypts_like_python_original() {
        // 与 encrypt.py 中的示例明文/密钥保持一致。
        assert_eq!(
            aes_encrypt("zyx/020305").unwrap(),
            "5ZTBUxmD+OY7LL1nzUUz+g=="
        );
    }
}
