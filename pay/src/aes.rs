//! copyright © ecdata.cn 2021 - present
//! 支付报文加解密，对齐 Kernel\Support\AesGcm 与 AesEcb

use base64::{decode, encode};
use md5::compute as md5_compute;
use openssl::symm::{Cipher, Crypter, Mode};
use wechat_sdk::WechatResult as Result;

/// GCM 认证标签长度
pub const GCM_TAG_LENGTH: usize = 16;

/// 去掉 PKCS7 填充
pub fn pkcs7_unpad(data: &[u8]) -> &[u8] {
    match data.last() {
        Some(&pad) if pad as usize > 0 && (pad as usize) <= data.len() => {
            let pad = pad as usize;
            if data[data.len() - pad..].iter().all(|&byte| byte as usize == pad) {
                return &data[..data.len() - pad];
            }
            data
        }
        _ => data,
    }
}

fn base64_decode(ciphertext: &str) -> Result<Vec<u8>> {
    decode(ciphertext).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Invalid ciphertext: {}", e),
        }
    })
}

fn utf8(data: Vec<u8>) -> Result<String> {
    String::from_utf8(data).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Invalid utf8 payload: {}", e),
        }
    })
}

/// AES-256-GCM 解密，返回明文
pub fn aes_gcm_decrypt(
    ciphertext: &str,
    key: &str,
    nonce: &str,
    associated_data: &str,
) -> Result<String> {
    let raw = base64_decode(ciphertext)?;

    if raw.len() <= GCM_TAG_LENGTH {
        return Err(error! {code: -1, msg: "Invalid ciphertext length."});
    }

    let (data, tag) = raw.split_at(raw.len() - GCM_TAG_LENGTH);

    let mut crypter = Crypter::new(
        Cipher::aes_256_gcm(),
        Mode::Decrypt,
        key.as_bytes(),
        Some(nonce.as_bytes()),
    )
    .map_err(|e| {
        error! {
            code: -1,
            msg: format!("Init aes-gcm failed: {}", e),
        }
    })?;

    crypter.set_tag(tag).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Set gcm tag failed: {}", e),
        }
    })?;

    if !associated_data.is_empty() {
        crypter.aad_update(associated_data.as_bytes()).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Update aad failed: {}", e),
            }
        })?;
    }

    let mut plaintext = vec![0; data.len() + 16];
    let mut count = crypter.update(data, &mut plaintext).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Decrypt failed: {}", e),
        }
    })?;

    count += crypter.finalize(&mut plaintext[count..]).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Failed to decrypt payload: {}", e),
        }
    })?;
    plaintext.truncate(count);

    utf8(plaintext)
}

/// AES-256-GCM 加密，输出 base64(ciphertext||tag)
pub fn aes_gcm_encrypt(
    plaintext: &str,
    key: &str,
    nonce: &str,
    associated_data: &str,
) -> Result<String> {
    let data = plaintext.as_bytes();

    let mut crypter = Crypter::new(
        Cipher::aes_256_gcm(),
        Mode::Encrypt,
        key.as_bytes(),
        Some(nonce.as_bytes()),
    )
    .map_err(|e| {
        error! {
            code: -1,
            msg: format!("Init aes-gcm failed: {}", e),
        }
    })?;

    if !associated_data.is_empty() {
        crypter.aad_update(associated_data.as_bytes()).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Update aad failed: {}", e),
            }
        })?;
    }

    let mut ciphertext = vec![0; data.len() + 16];
    let mut count = crypter.update(data, &mut ciphertext).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Encrypt failed: {}", e),
        }
    })?;

    count += crypter.finalize(&mut ciphertext[count..]).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Encrypt failed: {}", e),
        }
    })?;
    ciphertext.truncate(count);

    let mut tag = [0; GCM_TAG_LENGTH];
    crypter.get_tag(&mut tag).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Get gcm tag failed: {}", e),
        }
    })?;

    ciphertext.extend_from_slice(&tag);
    Ok(encode(ciphertext))
}

/// AES-256-ECB 解密（v2 退款通知 req_info），key 取 md5(APIv2 密钥)
pub fn aes_ecb_decrypt(ciphertext: &str, key: &str) -> Result<String> {
    let data = base64_decode(ciphertext)?;
    let md5_key = format!("{:x}", md5_compute(key.as_bytes()));

    let mut crypter = Crypter::new(Cipher::aes_256_ecb(), Mode::Decrypt, md5_key.as_bytes(), None)
        .map_err(|e| {
            error! {
                code: -1,
                msg: format!("Init aes-ecb failed: {}", e),
            }
        })?;
    crypter.pad(false);

    let mut plaintext = vec![0; data.len() + 16];
    let mut count = crypter.update(&data, &mut plaintext).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Decrypt failed: {}", e),
        }
    })?;
    count += crypter.finalize(&mut plaintext[count..]).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Decrypt failed: {}", e),
        }
    })?;
    plaintext.truncate(count);

    utf8(pkcs7_unpad(&plaintext).to_vec())
}

/// AES-256-ECB 加密（自动 PKCS7 填充），用于测试与 v2 报文
pub fn aes_ecb_encrypt(plaintext: &str, key: &str) -> Result<String> {
    let md5_key = format!("{:x}", md5_compute(key.as_bytes()));

    let mut crypter = Crypter::new(Cipher::aes_256_ecb(), Mode::Encrypt, md5_key.as_bytes(), None)
        .map_err(|e| {
            error! {
                code: -1,
                msg: format!("Init aes-ecb failed: {}", e),
            }
        })?;

    let data = plaintext.as_bytes();
    let mut ciphertext = vec![0; data.len() + 32];
    let mut count = crypter.update(data, &mut ciphertext).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Encrypt failed: {}", e),
        }
    })?;
    count += crypter.finalize(&mut ciphertext[count..]).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Encrypt failed: {}", e),
        }
    })?;
    ciphertext.truncate(count);

    Ok(encode(ciphertext))
}

#[cfg(test)]
mod tests {
    use super::*;

    const V3_KEY: &str = "0123456789abcdef0123456789abcdef";
    const V2_KEY: &str = "v2-secret-key";

    #[test]
    fn gcm_roundtrip() {
        let plaintext = r#"{"out_trade_no":"1217752501201407033233368018"}"#;

        let ciphertext = aes_gcm_encrypt(plaintext, V3_KEY, "nonce-1", "transaction").unwrap();
        assert_eq!(
            aes_gcm_decrypt(&ciphertext, V3_KEY, "nonce-1", "transaction").unwrap(),
            plaintext
        );
    }

    #[test]
    fn gcm_rejects_wrong_association() {
        let ciphertext = aes_gcm_encrypt("{}", V3_KEY, "nonce", "aad").unwrap();
        assert!(aes_gcm_decrypt(&ciphertext, V3_KEY, "nonce", "other").is_err());
    }

    #[test]
    fn gcm_supports_empty_associated_data() {
        let ciphertext = aes_gcm_encrypt("hello", V3_KEY, "nonce-2", "").unwrap();
        assert_eq!(
            aes_gcm_decrypt(&ciphertext, V3_KEY, "nonce-2", "").unwrap(),
            "hello"
        );
    }

    #[test]
    fn pkcs7_unpad_removes_padding() {
        let mut data = b"abc".to_vec();
        data.extend_from_slice(&[5, 5, 5, 5, 5]);
        assert_eq!(pkcs7_unpad(&data), b"abc");
        assert_eq!(pkcs7_unpad(b"abc"), b"abc");
    }

    #[test]
    fn ecb_roundtrip() {
        let plaintext = "<xml><out_trade_no>123</out_trade_no></xml>";

        let ciphertext = aes_ecb_encrypt(plaintext, V2_KEY).unwrap();
        assert_eq!(aes_ecb_decrypt(&ciphertext, V2_KEY).unwrap(), plaintext);
    }
}
