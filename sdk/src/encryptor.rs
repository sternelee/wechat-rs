//! copyright © ecdata.cn 2021 - present
//! 消息加解密，对齐 EasyWeChat\Kernel\Encryptor
//! DOC https://developers.weixin.qq.com/doc/offiaccount/Message_Management/Message_encryption_and_decryption_instructions.html
//! created by shaipe 20210228

use crate::signature::create_signature;
use crate::strutil::{random, random_nonce};
use crate::{current_timestamp, WechatResult as Result};
use crypto::buffer::{BufferResult, ReadBuffer, WriteBuffer};
use crypto::{aes, blockmodes, buffer};
use std::collections::HashMap;

/// AES 分块大小
const BLOCK_SIZE: usize = 16;

/// 加密结果，对齐 Encryptor::encryptAsArray 返回结构
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedMessage {
    pub ciphertext: String,
    pub signature: String,
    pub timestamp: u64,
    pub nonce: String,
}

/// 微信消息加解密器
#[derive(Debug, Clone)]
pub struct Encryptor {
    app_id: String,
    token: String,
    aes_key: Vec<u8>,
    receive_id: Option<String>,
}

impl Encryptor {
    /// 创建加解密器
    /// - app_id: 公众号/小程序 appId
    /// - token: 消息校验 token
    /// - aes_key: 43 位 EncodingAESKey（会自动补 `=`）
    /// - receive_id: 校验接收方 id，默认使用 app_id
    pub fn new(
        app_id: &str,
        token: &str,
        aes_key: &str,
        receive_id: Option<&str>,
    ) -> Result<Encryptor> {
        let mut key = aes_key.trim().to_owned();
        // 补齐 base64 padding
        while key.len() % 4 != 0 {
            key.push('=');
        }
        let aes_key_bytes = match base64::decode(&key) {
            Ok(v) => v,
            Err(e) => return Err(error! {code: -40004, msg: format!("Invalid AESKey: {:?}", e)}),
        };

        Ok(Encryptor {
            app_id: app_id.to_owned(),
            token: token.to_owned(),
            aes_key: aes_key_bytes,
            receive_id: Some(match receive_id {
                Some(v) => v.to_owned(),
                None => app_id.to_owned(),
            }),
        })
    }

    /// 获取 token
    pub fn get_token(&self) -> &str {
        &self.token
    }

    /// 计算签名
    pub fn create_signature<S: AsRef<str>>(&self, attributes: &[S]) -> String {
        create_signature(attributes)
    }

    /// 加密，返回 XML 或 JSON 字符串
    pub fn encrypt(
        &self,
        plaintext: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
        message_type: &str,
    ) -> Result<String> {
        if message_type == "xml" {
            self.encrypt_as_xml(plaintext, nonce, timestamp)
        } else {
            self.encrypt_as_json(plaintext, nonce, timestamp)
        }
    }

    /// 加密并返回 XML
    pub fn encrypt_as_xml(
        &self,
        plaintext: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<String> {
        let encrypted = self.encrypt_as_array(plaintext, nonce, timestamp)?;
        Ok(format!(
            "<xml>\n\
             <Encrypt><![CDATA[{encrypt}]]></Encrypt>\n\
             <MsgSignature><![CDATA[{signature}]]></MsgSignature>\n\
             <TimeStamp>{timestamp}</TimeStamp>\n\
             <Nonce><![CDATA[{nonce}]]></Nonce>\n\
             </xml>",
            encrypt = encrypted.ciphertext,
            signature = encrypted.signature,
            timestamp = encrypted.timestamp,
            nonce = encrypted.nonce,
        ))
    }

    /// 加密并返回 JSON
    pub fn encrypt_as_json(
        &self,
        plaintext: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<String> {
        let encrypted = self.encrypt_as_array(plaintext, nonce, timestamp)?;
        Ok(serde_json::json!({
            "encrypt": encrypted.ciphertext,
            "msgsignature": encrypted.signature,
            "timestamp": encrypted.timestamp,
            "nonce": encrypted.nonce,
        })
        .to_string())
    }

    /// 加密并返回结构体，对齐 Encryptor::encryptAsArray
    pub fn encrypt_as_array(
        &self,
        plaintext: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<EncryptedMessage> {
        // 16 位随机串 + 4 字节网络序长度 + 明文 + appId
        let mut raw: Vec<u8> = random(BLOCK_SIZE).into_bytes();
        raw.extend((plaintext.len() as u32).to_be_bytes());
        raw.extend(plaintext.as_bytes());
        raw.extend(self.receive_id_string().as_bytes());

        let padded = pkcs7_padding(&raw, self.aes_key.len().max(BLOCK_SIZE));
        let cipher = aes256_cbc_encrypt_no_padding(&padded, &self.aes_key, &self.aes_key[..BLOCK_SIZE])
            .map_err(|e| {
                error! {code: -40006, msg: format!("Encrypt AES error: {:?}", e)}
            })?;
        let ciphertext = base64::encode(&cipher);

        let nonce = nonce
            .map(|s| s.to_owned())
            .unwrap_or_else(random_nonce);
        let timestamp = timestamp.unwrap_or_else(current_timestamp);

        Ok(EncryptedMessage {
            signature: self.create_signature(&[
                self.token.as_str(),
                &timestamp.to_string(),
                nonce.as_str(),
                ciphertext.as_str(),
            ]),
            ciphertext,
            timestamp,
            nonce,
        })
    }

    /// 解密消息，签名不匹配或 appId 不匹配会报错
    pub fn decrypt(
        &self,
        ciphertext: &str,
        msg_signature: &str,
        nonce: &str,
        timestamp: &str,
    ) -> Result<String> {
        let signature = self.create_signature(&[self.token.as_str(), timestamp, nonce, ciphertext]);
        if !crate::signature::constant_time_eq(&signature, msg_signature) {
            return Err(error! {code: -40001, msg: "Invalid Signature."});
        }

        let cipher = base64::decode(ciphertext)
            .map_err(|e| error! {code: -40010, msg: format!("Base64 decode error: {:?}", e)})?;
        let plain = if self.aes_key.len() >= 32 {
            aes256_cbc_decrypt_no_padding(&cipher, &self.aes_key, &self.aes_key[..BLOCK_SIZE])
        } else {
            aes128_cbc_decrypt_no_padding(&cipher, &self.aes_key, &self.aes_key[..BLOCK_SIZE])
        }
        .map_err(|e| error! {code: -40007, msg: format!("Decrypt AES error: {:?}", e)})?;

        let plain = pkcs7_unpadding(&plain, self.aes_key.len().max(BLOCK_SIZE));
        if plain.len() < BLOCK_SIZE + 4 {
            return Err(error! {code: -41003, msg: "Illegal buffer."});
        }

        let content_length =
            u32::from_be_bytes([plain[16], plain[17], plain[18], plain[19]]) as usize;
        if plain.len() < 20 + content_length {
            return Err(error! {code: -41003, msg: "Illegal buffer."});
        }
        let content = &plain[20..20 + content_length];

        if let Some(receive_id) = &self.receive_id {
            let from_id = String::from_utf8_lossy(&plain[20 + content_length..])
                .trim()
                .to_owned();
            if !receive_id.is_empty() && from_id != *receive_id {
                return Err(error! {code: -40005, msg: "Invalid appId."});
            }
        }

        String::from_utf8(content.to_vec())
            .map_err(|e| error! {code: -41003, msg: format!("Illegal buffer: {:?}", e)})
    }

    /// 从 XML 中取出 Encrypt 节点并解密
    pub fn decrypt_xml(&self, xml: &str, query: &HashMap<String, String>) -> Result<String> {
        let package = crate::xmlutil::parse(xml);
        let doc = package.as_document();
        let ciphertext = crate::xmlutil::evaluate(&doc, "//xml/Encrypt/text()").string();

        let nonce = query.get("nonce").cloned().unwrap_or_default();
        let timestamp = query.get("timestamp").cloned().unwrap_or_default();
        let signature = query.get("msg_signature").cloned().unwrap_or_default();

        self.decrypt(&ciphertext, &signature, &nonce, &timestamp)
    }

    fn receive_id_string(&self) -> String {
        self.receive_id.clone().unwrap_or_else(|| self.app_id.clone())
    }
}

/// PKCS#7 填充，对齐 EasyWeChat\Support\Pkcs7::padding
pub fn pkcs7_padding(data: &[u8], block_size: usize) -> Vec<u8> {
    let block_size = if block_size == 0 { BLOCK_SIZE } else { block_size };
    let pad = block_size - (data.len() % block_size);
    let mut out = Vec::with_capacity(data.len() + pad);
    out.extend_from_slice(data);
    for _ in 0..pad {
        out.push(pad as u8);
    }
    out
}

/// PKCS#7 去除填充，对齐 EasyWeChat\Support\Pkcs7::unpadding
pub fn pkcs7_unpadding(data: &[u8], block_size: usize) -> Vec<u8> {
    let block_size = if block_size == 0 { BLOCK_SIZE } else { block_size };
    if data.is_empty() {
        return Vec::new();
    }
    let pad = data[data.len() - 1] as usize;
    if pad == 0 || pad > block_size || pad > data.len() {
        return data.to_vec();
    }
    data[..data.len() - pad].to_vec()
}

fn aes256_cbc_encrypt_no_padding(
    data: &[u8],
    key: &[u8],
    iv: &[u8],
) -> std::result::Result<Vec<u8>, crypto::symmetriccipher::SymmetricCipherError> {
    cbc_encrypt(aes::KeySize::KeySize256, data, key, iv)
}

fn cbc_encrypt(
    key_size: aes::KeySize,
    data: &[u8],
    key: &[u8],
    iv: &[u8],
) -> std::result::Result<Vec<u8>, crypto::symmetriccipher::SymmetricCipherError> {
    let mut encryptor = aes::cbc_encryptor(key_size, key, iv, blockmodes::NoPadding);
    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

    loop {
        let result = encryptor.encrypt(&mut read_buffer, &mut write_buffer, true)?;
        final_result.extend(write_buffer.take_read_buffer().take_remaining().iter().copied());
        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => {}
        }
    }
    Ok(final_result)
}

fn aes256_cbc_decrypt_no_padding(
    data: &[u8],
    key: &[u8],
    iv: &[u8],
) -> std::result::Result<Vec<u8>, crypto::symmetriccipher::SymmetricCipherError> {
    cbc_decrypt(aes::KeySize::KeySize256, data, key, iv)
}

fn aes128_cbc_decrypt_no_padding(
    data: &[u8],
    key: &[u8],
    iv: &[u8],
) -> std::result::Result<Vec<u8>, crypto::symmetriccipher::SymmetricCipherError> {
    cbc_decrypt(aes::KeySize::KeySize128, data, key, iv)
}

fn cbc_decrypt(
    key_size: aes::KeySize,
    data: &[u8],
    key: &[u8],
    iv: &[u8],
) -> std::result::Result<Vec<u8>, crypto::symmetriccipher::SymmetricCipherError> {
    let mut decryptor = aes::cbc_decryptor(key_size, key, iv, blockmodes::NoPadding);
    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

    loop {
        let result = decryptor.decrypt(&mut read_buffer, &mut write_buffer, true)?;
        final_result.extend(write_buffer.take_read_buffer().take_remaining().iter().copied());
        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => {}
        }
    }
    Ok(final_result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AES_KEY: &str = "kdjCGGJKSRjjhESfPO5lTSWtYS0v5pQX47skCkZczio";
    const TOKEN: &str = "shaipe";
    const APP_ID: &str = "wxce775970ff046a47";

    #[test]
    fn pkcs7_roundtrip() {
        let data = b"hello wechat";
        let padded = pkcs7_padding(data, 32);
        assert_eq!(padded.len() % 32, 0);
        assert_eq!(pkcs7_unpadding(&padded, 32), data.to_vec());
    }

    #[test]
    fn encrypt_then_decrypt() {
        let enc = Encryptor::new(APP_ID, TOKEN, AES_KEY, None).unwrap();
        let msg = "<xml><Content><![CDATA[hello]]></Content></xml>";
        let encrypted = enc.encrypt_as_array(msg, Some("1234567890"), Some(1587121403)).unwrap();
        let decrypted = enc
            .decrypt(
                &encrypted.ciphertext,
                &encrypted.signature,
                &encrypted.nonce,
                &encrypted.timestamp.to_string(),
            )
            .unwrap();
        assert_eq!(decrypted, msg);
    }

    #[test]
    fn decrypt_rejects_bad_signature() {
        let enc = Encryptor::new(APP_ID, TOKEN, AES_KEY, None).unwrap();
        let encrypted = enc
            .encrypt_as_array("hello", Some("1234567890"), Some(1587121403))
            .unwrap();
        let err = enc.decrypt(
            &encrypted.ciphertext,
            "bad-signature",
            &encrypted.nonce,
            &encrypted.timestamp.to_string(),
        );
        assert!(err.is_err());
    }

    #[test]
    fn encrypt_as_xml_contains_nodes() {
        let enc = Encryptor::new(APP_ID, TOKEN, AES_KEY, None).unwrap();
        let xml = enc
            .encrypt_as_xml("hello", Some("1234567890"), Some(1587121403))
            .unwrap();
        assert!(xml.contains("<Encrypt>"));
        assert!(xml.contains("<MsgSignature>"));
        assert!(xml.contains("<Nonce>"));
    }
}
