//! copyright © ecdata.cn 2021 - present
//! JS-SDK 票据管理，对齐 EasyWeChat\OfficialAccount\JsApiTicket / Work\JsApiTicket
//! created by shaipe 20211012

use crate::{cache, constant::API_DOMAIN, signature, strutil, Client, WechatResult as Result};

/// 票据缓存前缀

/// 提前刷新时间（秒）
const REFRESH_AHEAD: u64 = 200;

/// JS-SDK 票据
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Ticket {
    /// 访问票据
    pub ticket: String,
    /// 获取时间
    pub create_time: u64,
    /// 有效期
    pub expires: i64,
}

/// jsapi_ticket 管理器
#[derive(Debug, Clone, Default)]
pub struct JsApiTicket {
    // 应用类型
    pub app_type: String,
    // 应用id
    pub app_id: String,
    // 应用密钥
    pub secret: String,
    // 自定义缓存 key
    pub key: Option<String>,
}

impl JsApiTicket {
    /// 创建实例
    pub fn new(app_type: &str, app_id: &str, secret: &str) -> JsApiTicket {
        JsApiTicket {
            app_type: app_type.to_owned(),
            app_id: app_id.to_owned(),
            secret: secret.to_owned(),
            key: None,
        }
    }

    /// 自定义缓存 key
    pub fn set_key(mut self, key: &str) -> Self {
        self.key = Some(key.to_owned());
        self
    }

    /// 缓存 key，对齐 easywechat `official_account.jsapi_ticket.{appId}`
    pub fn get_key(&self) -> String {
        if let Some(key) = &self.key {
            return key.clone();
        }
        // 对齐 PHP：{app_type}.jsapi_ticket.{appId}
        format!("{}.jsapi_ticket.{}", self.app_type, self.app_id)
    }

    /// 刷新票据，GET /cgi-bin/ticket/getticket?type=jsapi
    pub async fn refresh_ticket(&self) -> Result<Ticket> {
        let access_token = self.access_token().await?;
        let url = format!(
            "{}/cgi-bin/ticket/getticket?access_token={}&type=jsapi",
            API_DOMAIN, access_token
        );
        let data = Client::new().get_json(&url).await?;
        let ticket = match data["ticket"].as_str() {
            Some(v) => v.to_owned(),
            None => {
                return Err(error! {
                    code: -1,
                    msg: format!("Failed to get jssdk ticket: {}", data)
                })
            }
        };
        let expires = data["expires_in"].as_i64().unwrap_or(7200);
        Ok(Ticket {
            ticket,
            create_time: crate::current_timestamp(),
            expires,
        })
    }

    /// 获取 access_token
    pub async fn access_token(&self) -> Result<String> {
        let token = crate::AccessToken::new(&self.app_type, &self.app_id, &self.secret);
        token.get_token().await
    }

    /// 写入缓存
    pub fn set(&self, val: Ticket) {
        let ttl = if val.expires > 0 { val.expires as u64 } else { 7200 };

        cache::set(&self.get_key(), &val, ttl);
    }

    /// 读取缓存
    pub fn get_cached(&self) -> Option<Ticket> {
        cache::get(&self.get_key())
    }

    /// 清除缓存
    pub fn forget(&self) {
        cache::forget(&self.get_key());
    }

    /// 判断票据是否过期
    pub fn is_expired(&self, ticket: &Ticket) -> bool {
        let now = crate::current_timestamp();
        let expires = if ticket.expires <= 0 { 7200 } else { ticket.expires as u64 };
        if now < ticket.create_time {
            return true;
        }
        now - ticket.create_time + REFRESH_AHEAD >= expires
    }

    /// 获取 jsapi_ticket，缓存失效自动刷新
    pub async fn get_ticket(&self) -> Result<String> {
        if let Some(ticket) = self.get_cached() {
            if !ticket.ticket.is_empty() && !self.is_expired(&ticket) {
                return Ok(ticket.ticket);
            }
        }
        let ticket = self.refresh_ticket().await?;
        self.set(ticket.clone());
        Ok(ticket.ticket)
    }

    /// 生成 JS-SDK 配置签名，对齐 JsApiTicket::configSignature
    pub async fn config_signature(
        &self,
        url: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<serde_json::Value> {
        let ticket = self.get_ticket().await?;
        let nonce = nonce
            .map(|s| s.to_owned())
            .unwrap_or_else(strutil::random_nonce);
        let timestamp = timestamp.unwrap_or_else(crate::current_timestamp);
        let signature = signature::jsapi_signature(&ticket, &nonce, timestamp, url);
        Ok(serde_json::json!({
            "url": url,
            "nonceStr": nonce,
            "timestamp": timestamp,
            "appId": self.app_id,
            "signature": signature,
        }))
    }

    /// 生成完整的 JSSDK config（含 jsApiList/debug/openTagList），对齐 OfficialAccount\Utils::buildJsSdkConfig
    pub async fn build_config(
        &self,
        url: &str,
        js_api_list: Vec<String>,
        open_tag_list: Vec<String>,
        debug: bool,
    ) -> Result<serde_json::Value> {
        let mut config = self.config_signature(url, None, None).await?;
        config["jsApiList"] = serde_json::json!(js_api_list);
        config["openTagList"] = serde_json::json!(open_tag_list);
        config["debug"] = serde_json::json!(debug);
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_key_contains_app_id() {
        let t = JsApiTicket::new("mp", "wx123", "secret");
        assert!(t.get_key().contains("wx123"));
    }

    #[test]
    fn expired_detection_works() {
        let t = JsApiTicket::new("mp", "wx123", "secret");
        let fresh = Ticket {
            ticket: "tk".to_owned(),
            create_time: crate::current_timestamp(),
            expires: 7200,
        };
        assert!(!t.is_expired(&fresh));
    }

    #[test]
    fn cache_roundtrip() {
        let _guard = crate::cache::TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let t = JsApiTicket::new("mp", "wx_ticket_cache", "secret");
        t.set(Ticket {
            ticket: "tk-value".to_owned(),
            create_time: crate::current_timestamp(),
            expires: 7200,
        });
        assert_eq!(t.get_cached().unwrap().ticket, "tk-value");
    }
}
