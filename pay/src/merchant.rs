//! copyright © ecdata.cn 2021 - present
//! 商户信息，对齐 EasyWeChat\Pay\Merchant

use openssl::pkey::PKey;
use openssl::x509::X509;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use wechat_sdk::WechatResult as Result;

/// 读取 PEM 内容，若不是 PEM 内容则当作文件路径读取
pub fn load_pem(value: &str) -> Result<String> {
    if value.contains("-----BEGIN") {
        return Ok(value.to_owned());
    }

    fs::read_to_string(value).map_err(|e| {
        error! {
            code: -1,
            msg: format!("Read pem file `{}` failed: {}", value, e),
        }
    })
}

/// 商户信息
#[derive(Debug, Clone)]
pub struct Merchant {
    /// 商户号
    pub mch_id: String,
    /// 商户 API 私钥（PEM）
    pub private_key: String,
    /// 商户 API 证书（PEM）
    pub certificate: String,
    /// APIv3 密钥
    pub secret_key: String,
    /// APIv2 密钥
    pub v2_secret_key: Option<String>,
    /// 平台证书或微信支付公钥：serial => PEM
    pub platform_certs: HashMap<String, String>,
}

impl Merchant {
    /// 创建商户
    pub fn new(
        mch_id: impl Into<String>,
        private_key: impl Into<String>,
        certificate: impl Into<String>,
        secret_key: impl Into<String>,
        v2_secret_key: Option<String>,
    ) -> Merchant {
        Merchant {
            mch_id: mch_id.into(),
            private_key: private_key.into(),
            certificate: certificate.into(),
            secret_key: secret_key.into(),
            v2_secret_key,
            platform_certs: HashMap::new(),
        }
    }

    /// 从配置创建，对齐 Application::getMerchant
    pub fn from_value(config: &Value) -> Result<Merchant> {
        let get = |key: &str| config[key].as_str().map(|v| v.to_owned());

        let mch_id = get("mch_id")
            .or_else(|| config["mch_id"].as_i64().map(|v| v.to_string()))
            .ok_or_else(|| error! {code: -1, msg: "Missing config `mch_id`."})?;
        let private_key = get("private_key")
            .ok_or_else(|| error! {code: -1, msg: "Missing config `private_key`."})?;
        let certificate = get("certificate")
            .ok_or_else(|| error! {code: -1, msg: "Missing config `certificate`."})?;
        let secret_key = get("secret_key")
            .ok_or_else(|| error! {code: -1, msg: "Missing config `secret_key`."})?;

        let mut merchant = Merchant::new(
            mch_id,
            load_pem(&private_key)?,
            load_pem(&certificate)?,
            secret_key,
            get("v2_secret_key"),
        );

        if let Some(certs) = config["platform_certs"].as_object() {
            for (serial, value) in certs {
                if let Some(pem) = value.as_str() {
                    merchant
                        .platform_certs
                        .insert(serial.to_owned(), load_pem(pem)?);
                }
            }
        }

        Ok(merchant)
    }

    /// 添加平台证书
    pub fn add_platform_cert(mut self, serial: impl Into<String>, pem: impl Into<String>) -> Self {
        self.platform_certs.insert(serial.into(), pem.into());
        self
    }

    /// 商户证书序列号（大写十六进制），对齐 PublicKey::getSerialNo
    pub fn certificate_serial(&self) -> Result<String> {
        let cert = X509::from_pem(self.certificate.as_bytes()).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Parse certificate failed: {}", e),
            }
        })?;

        let hex = cert
            .serial_number()
            .to_bn()
            .and_then(|bn| bn.to_hex_str())
            .map_err(|e| {
                error! {
                    code: -1,
                    msg: format!("Read certificate serial failed: {}", e),
                }
            })?;

        Ok(hex.to_string().to_uppercase())
    }

    /// 私钥对象
    pub fn private_key_pkey(&self) -> Result<PKey<openssl::pkey::Private>> {
        PKey::private_key_from_pem(self.private_key.as_bytes()).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Parse private key failed: {}", e),
            }
        })
    }

    /// 平台证书公钥，对齐 getPlatformCert
    pub fn platform_cert(&self, serial: &str) -> Option<&String> {
        self.platform_certs.get(serial)
    }

    /// 第一个平台证书（在线加密用）
    pub fn first_platform_cert(&self) -> Option<(&String, &String)> {
        self.platform_certs.iter().next()
    }

    /// V2 密钥，缺失时报错
    pub fn require_v2_secret_key(&self) -> Result<&String> {
        self.v2_secret_key
            .as_ref()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| error! {code: -1, msg: "Missing V2 API key."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merchant_from_value_requires_keys() {
        let err = Merchant::from_value(&serde_json::json!({"mch_id": "1"})).unwrap_err();
        assert!(format!("{:?}", err).contains("private_key"));
    }

    #[test]
    fn merchant_holds_platform_certs() {
        let merchant = Merchant::new("1900000109", "key", "cert", "v3key", Some("v2key".into()))
            .add_platform_cert("SERIAL1", "-----BEGIN PUBLIC KEY-----");

        assert_eq!(
            merchant.platform_cert("SERIAL1").map(|v| v.as_str()),
            Some("-----BEGIN PUBLIC KEY-----")
        );
        assert!(merchant.first_platform_cert().is_some());
        assert_eq!(
            merchant.require_v2_secret_key().unwrap(),
            "v2key"
        );
    }

    #[test]
    fn load_pem_keeps_inline_content() {
        assert_eq!(load_pem("-----BEGIN X-----").unwrap(), "-----BEGIN X-----");
    }
}
