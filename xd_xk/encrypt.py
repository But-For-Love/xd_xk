"""AES 加密模块 — 用于密码加密."""

import base64

from Cryptodome.Cipher import AES

# ── 常量 ──────────────────────────────────────────────────────────
_AES_KEY = "MWMqg2tPcDkxcm11"
_AES_BLOCK_SIZE = 16


class AESCipher:
    """AES-ECB 加密器（PKCS7 填充）.

    策略模式：密钥通过构造注入，可替换密钥或算法实现.
    """

    def __init__(self, key: str) -> None:
        self.key = key.encode("utf-8")

    @staticmethod
    def _pkcs7_pad(data: bytes, block_size: int = _AES_BLOCK_SIZE) -> bytes:
        """PKCS7 填充 — 直接操作字节，避免 str/bytes 混用."""
        pad_len = block_size - (len(data) % block_size)
        return data + bytes([pad_len] * pad_len)

    def encrypt(self, plaintext: str) -> str:
        """AES-ECB 加密，返回 Base64 字符串."""
        cipher = AES.new(self.key, AES.MODE_ECB)
        data = plaintext.encode("utf-8")
        padded = self._pkcs7_pad(data)
        encrypted = cipher.encrypt(padded)
        return base64.b64encode(encrypted).decode("ascii")


def AES_encrypt(text: str) -> str:
    """加密密码明文（便捷函数，保持向后兼容）."""
    return AESCipher(key=_AES_KEY).encrypt(text)
