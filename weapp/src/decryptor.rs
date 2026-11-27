//! copyright © ecdata.cn 2021 - present
//! 小程序用户数据解密，对齐 EasyWeChat\MiniApp\Decryptor
//! created by shaipe 20210228

use serde_json::Value;
use wechat_sdk::{aes128_cbc_decrypt, WechatResult as Result};

/// 小程序加解密工具
pub struct Decryptor;

impl Decryptor {
    /// 解密用户数据（AES-128-CBC + PKCS#7）
    ///
    /// - session_key: 登录时获取的 session_key（base64）
    /// - iv: 加密向量（base64）
    /// - ciphertext: 待解密数据（base64）
    ///
    /// 对齐 Decryptor::decrypt
    pub fn decrypt(session_key: &str, iv: &str, ciphertext: &str) -> Result<Value> {
        use base64::decode;

        let key = decode(session_key)
            .map_err(|e| error! {code: -1, msg: format!("Invalid session_key: {}", e)})?;
        let iv_bytes =
            decode(iv).map_err(|e| error! {code: -1, msg: format!("Invalid iv: {}", e)})?;
        let data = decode(ciphertext)
            .map_err(|e| error! {code: -1, msg: format!("Invalid ciphertext: {}", e)})?;

        let plain = aes128_cbc_decrypt(&data, &key, &iv_bytes)
            .map_err(|e| error! {code: -1, msg: format!("Decrypt failed: {:?}", e)})?;

        // aes128_cbc_decrypt 使用 NoPadding，需要手动去掉 PKCS#7 填充
        let plain = pkcs7_unpad(&plain);

        let text = std::str::from_utf8(plain)
            .map_err(|e| error! {code: -1, msg: format!("Invalid UTF-8: {}", e)})?;

        serde_json::from_str(text)
            .map_err(|e| error! {code: -1, msg: format!("Invalid payload: {}", e)})
    }

    /// 解密手机号（旧版 encryptedData + iv）并返回 phoneNumber 所在的 JSON
    pub fn decrypt_phone(encrypted_data: &str, session_key: &str, iv: &str) -> Result<Value> {
        Self::decrypt(session_key, iv, encrypted_data)
    }
}

/// 去掉 PKCS#7 填充
pub fn pkcs7_unpad(data: &[u8]) -> &[u8] {
    if let Some(&last) = data.last() {
        let pad = last as usize;
        if pad >= 1 && pad <= 32 && pad <= data.len() {
            let (body, tail) = data.split_at(data.len() - pad);
            if tail.iter().all(|&b| b == last) {
                return body;
            }
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use wechat_sdk::aes128_cbc_encrypt;

    #[test]
    fn decrypt_roundtrip() {
        use base64::encode;

        let key = b"0123456789abcdef";
        let iv = b"abcdef0123456789";
        let plain = br#"{"phoneNumber":"13800000000"}"#;
        let cipher = aes128_cbc_encrypt(plain, key, iv).unwrap();

        let value = Decryptor::decrypt(&encode(key), &encode(iv), &encode(cipher)).unwrap();
        assert_eq!(value["phoneNumber"], "13800000000");
    }

    #[test]
    fn unpad_handles_invalid_padding() {
        assert_eq!(pkcs7_unpad(b"abc"), b"abc");
        assert_eq!(pkcs7_unpad(b"abc\x02\x02"), b"abc");
    }
}
