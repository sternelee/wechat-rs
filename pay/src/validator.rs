//! copyright © ecdata.cn 2021 - present
//! 支付响应/回调验签，对齐 EasyWeChat\Pay\Validator 与 ResponseValidator

use crate::merchant::Merchant;
use base64::decode;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Verifier;
use std::collections::HashMap;
use wechat_sdk::WechatResult as Result;

/// 允许的时钟偏移（秒）
pub const MAX_ALLOWED_CLOCK_OFFSET: i64 = 300;
/// 时间戳头
pub const HEADER_TIMESTAMP: &str = "Wechatpay-Timestamp";
/// 随机串头
pub const HEADER_NONCE: &str = "Wechatpay-Nonce";
/// 证书序列号头
pub const HEADER_SERIAL: &str = "Wechatpay-Serial";
/// 签名头
pub const HEADER_SIGNATURE: &str = "Wechatpay-Signature";

/// 大小写无关地取头
pub fn get_header<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a String> {
    headers
        .get(name)
        .or_else(|| headers.get(&name.to_lowercase()))
        .or_else(|| {
            headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value)
        })
}

/// 支付验签，对齐 Validator::validate
#[derive(Debug)]
pub struct Validator<'a> {
    merchant: &'a Merchant,
}

impl<'a> Validator<'a> {
    pub fn new(merchant: &'a Merchant) -> Validator<'a> {
        Validator { merchant }
    }

    /// 校验回调签名
    ///
    /// - headers: 回调请求头
    /// - body: 原始请求体
    /// - now: 当前时间戳（秒），便于测试注入
    pub fn validate(&self, headers: &HashMap<String, String>, body: &str, now: i64) -> Result<()> {
        for name in [HEADER_TIMESTAMP, HEADER_NONCE, HEADER_SERIAL, HEADER_SIGNATURE] {
            if get_header(headers, name).map(|v| v.is_empty()).unwrap_or(true) {
                return Err(error! {code: -1, msg: format!("Missing Header: {}", name)});
            }
        }

        let timestamp = get_header(headers, HEADER_TIMESTAMP).map(|v| v.as_str()).unwrap_or_default();
        let nonce = get_header(headers, HEADER_NONCE).map(|v| v.as_str()).unwrap_or_default();
        let serial = get_header(headers, HEADER_SERIAL).map(|v| v.as_str()).unwrap_or_default();
        let signature = get_header(headers, HEADER_SIGNATURE).map(|v| v.as_str()).unwrap_or_default();

        // 对齐 Validator::validate：用 abs()，任何方向超过 300 秒都拒绝
        let offset = now - timestamp.parse::<i64>().unwrap_or_default();
        if offset.abs() > MAX_ALLOWED_CLOCK_OFFSET {
            return Err(error! {code: -1, msg: "Clock Offset Exceeded"});
        }

        let pem = self.merchant.platform_cert(serial).ok_or_else(|| {
            error! {
                code: -1,
                msg: format!("No platform certs found for serial: {}", serial),
            }
        })?;

        let message = format!("{}\n{}\n{}\n", timestamp, nonce, body);
        let signature = decode(signature).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Invalid Signature: {}", e),
            }
        })?;

        let key = PKey::public_key_from_pem(pem.as_bytes()).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Parse platform cert failed: {}", e),
            }
        })?;

        let mut verifier = Verifier::new(MessageDigest::sha256(), &key).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Create verifier failed: {}", e),
            }
        })?;

        verifier.update(message.as_bytes()).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Update verifier failed: {}", e),
            }
        })?;

        if !verifier.verify(&signature).unwrap_or(false) {
            return Err(error! {code: -1, msg: "Invalid Signature"});
        }

        Ok(())
    }
}

impl<'a> Validator<'a> {
    /// 校验响应，对齐 EasyWeChat\Pay\ResponseValidator：
    /// 要求 HTTP 200，并且携带 Wechatpay-* 头时按签名规则校验
    pub fn validate_response(
        &self,
        status: u16,
        headers: &[(String, String)],
        body: &str,
        now: i64,
    ) -> Result<()> {
        if status != 200 {
            return Err(error! {
                code: status as i32,
                msg: format!("Invalid response status: {}", status),
            });
        }

        let map: std::collections::HashMap<String, String> = headers
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        if get_header(&map, HEADER_SIGNATURE).is_none() {
            return Ok(());
        }

        self.validate(&map, body, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::sign_message;

    fn merchant() -> Merchant {
        use openssl::rsa::Rsa;

        let rsa = Rsa::generate(2048).unwrap();
        let private = String::from_utf8(rsa.private_key_to_pem().unwrap()).unwrap();
        let public = String::from_utf8(rsa.public_key_to_pem().unwrap()).unwrap();

        Merchant::new("1900000109", private, "cert", "v3key", Some("v2key".into()))
            .add_platform_cert("SERIAL", public)
    }

    fn signed_headers(merchant: &Merchant, body: &str, timestamp: i64) -> HashMap<String, String> {
        let message = format!("{}\n{}\n{}\n", timestamp, "nonce123", body);
        let signature = sign_message(merchant, &message).unwrap();

        let mut headers = HashMap::new();
        headers.insert(HEADER_TIMESTAMP.to_owned(), timestamp.to_string());
        headers.insert(HEADER_NONCE.to_owned(), "nonce123".to_owned());
        headers.insert(HEADER_SERIAL.to_owned(), "SERIAL".to_owned());
        headers.insert(HEADER_SIGNATURE.to_owned(), signature);

        headers
    }

    #[test]
    fn validate_accepts_valid_signature() {
        let merchant = merchant();
        let body = r#"{"id":"1"}"#;
        let headers = signed_headers(&merchant, body, 1_700_000_000);

        Validator::new(&merchant)
            .validate(&headers, body, 1_700_000_100)
            .unwrap();
    }

    #[test]
    fn validate_rejects_tampered_body() {
        let merchant = merchant();
        let headers = signed_headers(&merchant, r#"{"id":"1"}"#, 1_700_000_000);

        let err = Validator::new(&merchant)
            .validate(&headers, r#"{"id":"2"}"#, 1_700_000_100)
            .unwrap_err();

        assert!(format!("{:?}", err).contains("Invalid Signature"));
    }

    #[test]
    fn validate_rejects_missing_header() {
        let merchant = merchant();
        let err = Validator::new(&merchant)
            .validate(&HashMap::new(), "{}", 1_700_000_000)
            .unwrap_err();

        assert!(format!("{:?}", err).contains("Missing Header"));
    }

    #[test]
    fn validate_rejects_clock_offset() {
        let merchant = merchant();
        let body = "{}";
        let headers = signed_headers(&merchant, body, 1_700_000_000);

        let err = Validator::new(&merchant)
            .validate(&headers, body, 1_700_000_301)
            .unwrap_err();

        assert!(format!("{:?}", err).contains("Clock Offset Exceeded"));
    }

    #[test]
    fn validate_response_requires_200() {
        let merchant = merchant();
        let validator = Validator::new(&merchant);

        assert!(validator
            .validate_response(500, &[], "{}", 1_700_000_000)
            .is_err());
        // 无签名头时只校验状态码
        assert!(validator
            .validate_response(200, &[], "{}", 1_700_000_000)
            .is_ok());
    }
}
