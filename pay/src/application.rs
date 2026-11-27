//! copyright © ecdata.cn 2021 - present
//! 支付应用入口，对齐 EasyWeChat\Pay\Application

use crate::client::Client;
use crate::merchant::Merchant;
use crate::server::Server;
use crate::utils::Utils;
use crate::validator::Validator;
use serde_json::Value;
use wechat_sdk::WechatResult as Result;

/// 支付应用
#[derive(Debug, Clone)]
pub struct Application {
    config: Value,
    merchant: Merchant,
    client: Client,
}

impl Application {
    /// 从配置创建，必填 mch_id / secret_key / private_key / certificate
    pub fn new(config: Value) -> Result<Application> {
        let merchant = Merchant::from_value(&config)?;
        let client = Client::new(merchant.clone());

        Ok(Application {
            config,
            merchant,
            client,
        })
    }

    /// 原始配置
    pub fn config(&self) -> &Value {
        &self.config
    }

    /// 商户信息，对齐 getMerchant
    pub fn merchant(&self) -> &Merchant {
        &self.merchant
    }

    /// 工具类，对齐 getUtils
    pub fn utils(&self) -> Utils {
        Utils::new(self.merchant.clone())
    }

    /// 验签器，对齐 getValidator
    pub fn validator(&self) -> Validator<'_> {
        Validator::new(&self.merchant)
    }

    /// 回调服务器，对齐 getServer
    pub fn server(&self) -> Server {
        Server::new(self.merchant.clone())
    }

    /// HTTP 客户端，对齐 getClient
    pub fn client(&self) -> &Client {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_requires_merchant_config() {
        assert!(Application::new(serde_json::json!({"mch_id": "1"})).is_err());
    }

    #[test]
    fn application_builds_components() {
        let app = Application::new(serde_json::json!({
            "mch_id": "1900000109",
            "secret_key": "v3-secret-key",
            "private_key": "-----BEGIN PRIVATE KEY-----",
            "certificate": "-----BEGIN CERTIFICATE-----",
            "v2_secret_key": "chengduhongtuikeji20210911888888",
        }))
        .unwrap();

        assert_eq!(app.merchant().mch_id, "1900000109");
        assert_eq!(app.utils().merchant().mch_id, "1900000109");
        assert_eq!(app.client().merchant().mch_id, "1900000109");
        assert!(app.config().get("mch_id").is_some());
    }
}
