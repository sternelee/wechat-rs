//! copyright © ecdata.cn 2021 - present
//! 支付签名，对齐 EasyWeChat\Pay\Signature 与 LegacySignature

use crate::merchant::Merchant;
use base64::encode;
use hmac::{Hmac, Mac};
use md5::compute as md5_compute;
use openssl::hash::MessageDigest;
use openssl::sign::Signer;
use sha2::Sha256;
use std::collections::BTreeMap;
use wechat_sdk::strutil::random;
use wechat_sdk::WechatResult as Result;

/// 微信支付 v3 签名算法
pub const SCHEMA_SHA256: &str = "WECHATPAY2-SHA256-RSA2048";

/// 从完整 URL 中提取签名用的 path（含 query），对齐 Signature::createHeader
pub fn request_path(url: &str) -> String {
    let after_schema = match url.find("://") {
        Some(pos) => &url[pos + 3..],
        None => url,
    };

    match after_schema.find('/') {
        Some(pos) => after_schema[pos..].to_owned(),
        None => "/".to_owned(),
    }
}

/// RSA-SHA256 签名并 base64，对齐 Utils::createSignature
pub fn sign_message(merchant: &Merchant, message: &str) -> Result<String> {
    let key = merchant.private_key_pkey()?;
    let mut signer = Signer::new(MessageDigest::sha256(), &key).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Create signer failed: {}", e),
        }
    })?;

    signer.update(message.as_bytes()).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Update signer failed: {}", e),
        }
    })?;

    let signature = signer.sign_to_vec().map_err(|e| {
        error! {
            code: -1,
            msg: format!("Sign failed: {}", e),
        }
    })?;

    Ok(encode(signature))
}

/// 支付 v3 请求签名
#[derive(Debug)]
pub struct Signature<'a> {
    merchant: &'a Merchant,
}

impl<'a> Signature<'a> {
    pub fn new(merchant: &'a Merchant) -> Signature<'a> {
        Signature { merchant }
    }

    /// 生成 Authorization 头，对齐 createHeader
    pub fn create_header(&self, method: &str, url: &str, body: &str) -> Result<String> {
        let path = request_path(url);
        let timestamp = wechat_sdk::current_timestamp();
        let nonce = random(16);

        let message = format!(
            "{}\n{}\n{}\n{}\n{}\n",
            method.to_uppercase(),
            path,
            timestamp,
            nonce,
            body
        );

        let signature = sign_message(self.merchant, &message)?;
        let serial = self.merchant.certificate_serial()?;

        Ok(format!(
            "{} mchid=\"{}\",nonce_str=\"{}\",timestamp=\"{}\",serial_no=\"{}\",signature=\"{}\"",
            SCHEMA_SHA256, self.merchant.mch_id, nonce, timestamp, serial, signature
        ))
    }
}

/// 支付 v2 签名，对齐 LegacySignature
#[derive(Debug)]
pub struct LegacySignature<'a> {
    merchant: &'a Merchant,
}

impl<'a> LegacySignature<'a> {
    pub fn new(merchant: &'a Merchant) -> LegacySignature<'a> {
        LegacySignature { merchant }
    }

    /// 给参数补签名，返回带 sign 的参数
    pub fn sign(&self, params: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
        let mut attributes: BTreeMap<String, String> = BTreeMap::new();

        // 官方实现会补 nonce_str / sub_mch_id / sub_appid 默认值
        attributes.insert("nonce_str".to_owned(), random(16));
        attributes.extend(
            params
                .iter()
                .filter(|(_, value)| !value.is_empty())
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        attributes.retain(|key, value| key != "sign" && !value.is_empty());

        let sign = self.sign_attributes(&attributes)?;

        // 与 LegacySignature::sign 一致：返回的是「参与签名的参数集合」+ sign，
        // 因此自动补的 nonce_str 必须随请求一起发出去（微信 v2 必填）。
        let mut params = attributes;
        params.insert("sign".to_owned(), sign);

        Ok(params)
    }

    /// 对已排序参数签名，对齐 LegacySignature::sign 的 message 计算
    pub fn sign_attributes(&self, attributes: &BTreeMap<String, String>) -> Result<String> {
        let secret_key = self.merchant.require_v2_secret_key()?;

        let mut attrs: BTreeMap<String, String> = attributes
            .iter()
            .filter(|(key, value)| key.as_str() != "sign" && !value.is_empty())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        attrs.insert("key".to_owned(), secret_key.clone());

        let message = attrs
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join("&");

        let signature = if attributes
            .get("sign_type")
            .map(|value| value == "HMAC-SHA256")
            .unwrap_or(false)
        {
            let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes()).map_err(|e| {
                error! {
                    code: -1,
                    msg: format!("Init hmac failed: {}", e),
                }
            })?;

            mac.update(message.as_bytes());
            mac.finalize()
                .into_bytes()
                .iter()
                .map(|byte| format!("{:02x}", byte))
                .collect::<String>()
        } else {
            format!("{:x}", md5_compute(message.as_bytes()))
        };

        Ok(signature.to_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merchant() -> Merchant {
        Merchant::new("1900000109", "key", "cert", "v3key", Some("v2key".into()))
    }

    #[test]
    fn request_path_keeps_query() {
        assert_eq!(
            request_path("https://api.mch.weixin.qq.com/v3/pay/transactions/jsapi"),
            "/v3/pay/transactions/jsapi"
        );
        assert_eq!(
            request_path("https://api.mch.weixin.qq.com/v3/certificates?foo=1"),
            "/v3/certificates?foo=1"
        );
        assert_eq!(request_path("https://api.mch.weixin.qq.com"), "/");
    }

    #[test]
    fn legacy_signature_is_uppercase_md5() {
        let merchant = merchant();
        let signer = LegacySignature::new(&merchant);

        let mut attributes: BTreeMap<String, String> = BTreeMap::new();
        attributes.insert("appid".to_owned(), "wx".to_owned());
        attributes.insert("mch_id".to_owned(), "1900000109".to_owned());

        let signature = signer.sign_attributes(&attributes).unwrap();
        assert_eq!(signature, signature.to_uppercase());
        assert_eq!(signature.len(), 32);
    }

    #[test]
    fn legacy_sign_inserts_sign_field() {
        let merchant = merchant();
        let signer = LegacySignature::new(&merchant);

        let mut params: BTreeMap<String, String> = BTreeMap::new();
        params.insert("body".to_owned(), "test".to_owned());
        params.insert("total_fee".to_owned(), "1".to_owned());
        params.insert("empty".to_owned(), String::new());

        let signed = signer.sign(&params).unwrap();
        assert!(signed.contains_key("sign"));
        assert!(!signed.contains_key("empty"));
        // 与 PHP 一致：自动生成的 nonce_str 必须出现在返回参数里（v2 必填字段）
        assert_eq!(signed.get("nonce_str").map(String::len), Some(16));
        // 签名覆盖 nonce_str：重算一次应得到同一个 sign
        let resign = signer
            .sign_attributes(&signed.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap();
        assert_eq!(signed.get("sign"), Some(&resign));
    }

    #[test]
    fn legacy_signature_requires_v2_key() {
        let merchant = Merchant::new("1", "key", "cert", "v3key", None);
        let signer = LegacySignature::new(&merchant);
        assert!(signer.sign_attributes(&BTreeMap::new()).is_err());
    }

    #[test]
    fn hmac_sha256_signature() {
        let merchant = merchant();
        let signer = LegacySignature::new(&merchant);

        let mut attributes: BTreeMap<String, String> = BTreeMap::new();
        attributes.insert("sign_type".to_owned(), "HMAC-SHA256".to_owned());
        attributes.insert("appid".to_owned(), "wx".to_owned());

        let signature = signer.sign_attributes(&attributes).unwrap();
        assert_eq!(signature.len(), 64);
    }
}
