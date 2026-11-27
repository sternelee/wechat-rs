//! copyright © ecdata.cn 2021 - present
//! 支付工具，对齐 EasyWeChat\Pay\Utils

use crate::merchant::Merchant;
use crate::signature::{sign_message, LegacySignature};
use base64::encode;
use hmac::{Hmac, Mac};
use md5::compute as md5_compute;
use openssl::pkey::PKey;
use openssl::rsa::Padding;
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::BTreeMap;
use wechat_sdk::strutil::random;
use wechat_sdk::WechatResult as Result;

/// 支付工具
#[derive(Debug, Clone)]
pub struct Utils {
    merchant: Merchant,
}

impl Utils {
    pub fn new(merchant: Merchant) -> Utils {
        Utils { merchant }
    }

    /// 商户信息
    pub fn merchant(&self) -> &Merchant {
        &self.merchant
    }

    /// JSAPI/小程序调起支付参数，对齐 buildBridgeConfig
    pub fn build_bridge_config(
        &self,
        prepay_id: &str,
        app_id: &str,
        sign_type: &str,
    ) -> Result<Value> {
        let time_stamp = wechat_sdk::current_timestamp().to_string();
        let nonce_str = random(16);
        let package = format!("prepay_id={}", prepay_id);

        let message = format!(
            "{}\n{}\n{}\n{}\n",
            app_id, time_stamp, nonce_str, package
        );

        let pay_sign = if sign_type != "RSA" {
            let mut params: BTreeMap<String, String> = BTreeMap::new();
            params.insert("appId".to_owned(), app_id.to_owned());
            params.insert("timeStamp".to_owned(), time_stamp.clone());
            params.insert("nonceStr".to_owned(), nonce_str.clone());
            params.insert("package".to_owned(), package.clone());
            params.insert("signType".to_owned(), sign_type.to_owned());
            self.create_v2_signature(&params)?
        } else {
            sign_message(&self.merchant, &message)?
        };

        Ok(json!({
            "appId": app_id,
            "timeStamp": time_stamp,
            "nonceStr": nonce_str,
            "package": package,
            "signType": sign_type,
            "paySign": pay_sign,
        }))
    }

    /// JSSDK 调起支付参数（timeStamp 改名为 timestamp），对齐 buildSdkConfig
    pub fn build_sdk_config(&self, prepay_id: &str, app_id: &str, sign_type: &str) -> Result<Value> {
        let mut params = self.build_bridge_config(prepay_id, app_id, sign_type)?;
        let time_stamp = params["timeStamp"].clone();
        if let Some(object) = params.as_object_mut() {
            object.insert("timestamp".to_owned(), time_stamp);
            object.remove("timeStamp");
        }

        Ok(params)
    }

    /// 小程序调起支付参数，对齐 buildMiniAppConfig
    pub fn build_mini_app_config(
        &self,
        prepay_id: &str,
        app_id: &str,
        sign_type: &str,
    ) -> Result<Value> {
        self.build_bridge_config(prepay_id, app_id, sign_type)
    }

    /// APP 调起支付参数，对齐 buildAppConfig
    pub fn build_app_config(&self, prepay_id: &str, app_id: &str) -> Result<Value> {
        let timestamp = wechat_sdk::current_timestamp();
        let noncestr = random(16);
        let package = "Sign=WXPay".to_owned();

        let message = format!(
            "{}\n{}\n{}\n{}\n",
            app_id, timestamp, noncestr, prepay_id
        );
        let sign = sign_message(&self.merchant, &message)?;

        Ok(json!({
            "appid": app_id,
            "partnerid": self.merchant.mch_id,
            "prepayid": prepay_id,
            "noncestr": noncestr,
            "timestamp": timestamp,
            "package": package,
            "sign": sign,
        }))
    }

    /// RSA-SHA256 签名，对齐 createSignature
    pub fn create_signature(&self, message: &str) -> Result<String> {
        sign_message(&self.merchant, message)
    }

    /// 平台公钥加密（PKCS#1 OAEP），对齐 encryptWithRsaPublicKey
    pub fn encrypt_with_rsa_public_key(
        &self,
        plaintext: &str,
        serial: Option<&str>,
    ) -> Result<String> {
        let pem = match serial {
            Some(serial) => self.merchant.platform_cert(serial).cloned(),
            None => self
                .merchant
                .first_platform_cert()
                .map(|(_, pem)| pem.to_owned()),
        }
        .ok_or_else(|| error! {code: -1, msg: "Missing platform certificate."})?;

        let key = PKey::public_key_from_pem(pem.as_bytes()).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Parse platform cert failed: {}", e),
            }
        })?;
        let rsa = key.rsa().map_err(|e| {
            error! {
                code: -1,
                msg: format!("Read rsa public key failed: {}", e),
            }
        })?;

        let mut encrypted = vec![0; rsa.size() as usize];
        let len = rsa
            .public_encrypt(plaintext.as_bytes(), &mut encrypted, Padding::PKCS1_OAEP)
            .map_err(|e| {
                error! {
                    code: -1,
                    msg: format!("Encrypt failed: {}", e),
                }
            })?;
        encrypted.truncate(len);

        Ok(encode(encrypted))
    }

    /// v2 签名，对齐 createV2Signature
    pub fn create_v2_signature(&self, params: &BTreeMap<String, String>) -> Result<String> {
        let secret_key = self.merchant.require_v2_secret_key()?;

        let mut attributes: BTreeMap<String, String> = params
            .iter()
            .filter(|(key, value)| key.as_str() != "sign" && !value.is_empty())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        attributes.insert("key".to_owned(), secret_key.clone());

        let message = attributes
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join("&");

        let signature = if params
            .get("signType")
            .or_else(|| params.get("sign_type"))
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

    /// 直接委托 LegacySignature 补全参数签名
    pub fn sign_legacy(&self, params: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
        LegacySignature::new(&self.merchant).sign(params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openssl::rsa::Rsa;

    fn merchant() -> Merchant {
        let rsa = Rsa::generate(2048).unwrap();
        let private = String::from_utf8(rsa.private_key_to_pem().unwrap()).unwrap();
        let public = String::from_utf8(rsa.public_key_to_pem().unwrap()).unwrap();

        Merchant::new("1900000109", private, "cert", "secretKey/apiV3key", Some("chengduhongtuikeji20210911888888".into()))
            .add_platform_cert("SERIAL", public)
    }

    #[test]
    fn bridge_config_has_rsa_sign() {
        let utils = Utils::new(merchant());
        let config = utils.build_bridge_config("prepay-1", "wx-app", "RSA").unwrap();

        assert_eq!(config["package"], "prepay_id=prepay-1");
        assert_eq!(config["signType"], "RSA");
        assert!(config["paySign"].as_str().unwrap().len() > 100);
    }

    #[test]
    fn sdk_config_renames_timestamp() {
        let utils = Utils::new(merchant());
        let config = utils.build_sdk_config("prepay-1", "wx-app", "RSA").unwrap();

        assert!(config.get("timestamp").is_some());
        assert!(config.get("timeStamp").is_none());
    }

    #[test]
    fn app_config_uses_sign_key() {
        let utils = Utils::new(merchant());
        let config = utils.build_app_config("prepay-1", "wx-app").unwrap();

        assert_eq!(config["package"], "Sign=WXPay");
        assert_eq!(config["partnerid"], "1900000109");
        assert!(config.get("sign").is_some());
    }

    #[test]
    fn v2_signature_is_uppercase_md5() {
        let utils = Utils::new(merchant());
        let mut params: BTreeMap<String, String> = BTreeMap::new();
        params.insert("appId".to_owned(), "wx".to_owned());
        params.insert("signType".to_owned(), "MD5".to_owned());

        let signature = utils.create_v2_signature(&params).unwrap();
        assert_eq!(signature.len(), 32);
        assert_eq!(signature, signature.to_uppercase());
    }

    #[test]
    fn rsa_oaep_encrypt_roundtrip() {
        let merchant = merchant();
        let utils = Utils::new(merchant.clone());
        let encrypted = utils
            .encrypt_with_rsa_public_key("13800000000", Some("SERIAL"))
            .unwrap();

        let pem = merchant.platform_cert("SERIAL").unwrap();
        let key = PKey::private_key_from_pem(merchant.private_key.as_bytes()).unwrap();
        let rsa = key.rsa().unwrap();
        assert_eq!(pem.is_empty(), false);
        let data = base64::decode(encrypted).unwrap();
        let mut decrypted = vec![0; rsa.size() as usize];
        let len = rsa
            .private_decrypt(&data, &mut decrypted, Padding::PKCS1_OAEP)
            .unwrap();

        assert_eq!(&decrypted[..len], b"13800000000");
    }
}
