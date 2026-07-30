"""AES 加密模块 — 用于密码加密."""

import base64

from Cryptodome.Cipher import AES


class Encrypt:
    """AES-ECB 加密器（PKCS7 填充）."""

    def __init__(self, key: str) -> None:
        self.key = key.encode("utf-8")

    def pkcs7padding(self, text: str) -> str:
        """PKCS7 填充."""
        bs = 16
        length = len(text)
        bytes_length = len(text.encode("utf-8"))
        padding_size = length if (bytes_length == length) else bytes_length
        padding = bs - padding_size % bs
        padding_text = chr(padding) * padding
        self.coding = chr(padding)
        return text + padding_text

    def aes_encrypt(self, content: str) -> str:
        """AES-ECB 加密，返回 Base64 字符串."""
        cipher = AES.new(self.key, AES.MODE_ECB)
        content_padding = self.pkcs7padding(content)
        encrypt_bytes = cipher.encrypt(content_padding.encode("utf-8"))
        return str(base64.b64encode(encrypt_bytes), encoding="utf-8")


def AES_encrypt(text: str) -> str:
    """加密密码明文."""
    key = "MWMqg2tPcDkxcm11"
    return Encrypt(key=key).aes_encrypt(text)
