//! copyright © ecdata.cn 2021 - present
//! 企业微信 jsapi_ticket，对齐 EasyWeChat\Work\JsApiTicket
//! created by shaipe 20210228

use crate::token::AccessToken;
use wechat_sdk::constant::WORK_API_DOMAIN;
use serde_json::{json, Value};
use wechat_sdk::{cache, signature, strutil::random, Client, WechatResult as Result};

/// 企业微信 jsapi_ticket 管理器
#[derive(Debug, Clone)]
pub struct JsApiTicket {
    corp_id: String,
    secret: String,
    key: Option<String>,
    client: Client,
}

impl JsApiTicket {
    /// 创建 manager
    pub fn new<S: Into<String>>(corp_id: S, secret: S) -> JsApiTicket {
        JsApiTicket {
            corp_id: corp_id.into(),
            secret: secret.into(),
            key: None,
            client: Client::default(),
        }
    }

    /// 自定义缓存 key
    pub fn set_key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    /// 缓存 key，对齐 getKey
    pub fn get_key(&self) -> String {
        match &self.key {
            Some(key) => key.clone(),
            // 对齐 PHP：work.jsapi_ticket.{corpId}
            None => format!("work.jsapi_ticket.{}", self.corp_id),
        }
    }

    /// 应用级缓存 key，对齐 getAgentKey
    pub fn get_agent_key(&self, agent_id: i64) -> String {
        format!("{}.{}", self.get_key(), agent_id)
    }

    /// 获取 jsapi_ticket，对齐 getTicket
    pub async fn get_ticket(&self) -> Result<String> {
        let key = self.get_key();
        if let Some(ticket) = Self::get_cached(&key) {
            return Ok(ticket);
        }

        let access_token = AccessToken::new(&self.corp_id, &self.secret)
            .get_token()
            .await?;
        let url = Client::build_url(
            WORK_API_DOMAIN,
            "/cgi-bin/get_jsapi_ticket",
            &[("access_token", access_token.as_str())],
        );

        let res: Value = self.client.get_json(&url).await?;
        let ticket = res["ticket"].as_str().unwrap_or_default();

        if ticket.is_empty() {
            return Err(error! {
                code: res["errcode"].as_i64().unwrap_or(-1) as i32,
                msg: format!("Failed to get jssdk ticket: {}", res)
            });
        }

        Self::set_cached(&key, ticket);

        Ok(ticket.to_owned())
    }

    /// 获取应用级 ticket（agent_config），对齐 getAgentTicket
    pub async fn get_agent_ticket(&self, agent_id: i64) -> Result<String> {
        let key = self.get_agent_key(agent_id);
        if let Some(ticket) = Self::get_cached(&key) {
            return Ok(ticket);
        }

        let access_token = AccessToken::new(&self.corp_id, &self.secret)
            .get_token()
            .await?;
        let url = Client::build_url(
            WORK_API_DOMAIN,
            "/cgi-bin/ticket/get",
            &[
                ("access_token", access_token.as_str()),
                ("type", "agent_config"),
            ],
        );

        let res: Value = self.client.get_json(&url).await?;
        let ticket = res["ticket"].as_str().unwrap_or_default();

        if ticket.is_empty() {
            return Err(error! {
                code: res["errcode"].as_i64().unwrap_or(-1) as i32,
                msg: format!("Failed to get jssdk agentTicket: {}", res)
            });
        }

        Self::set_cached(&key, ticket);

        Ok(ticket.to_owned())
    }

    /// ticket 签名，对齐 getTicketSignature
    pub fn get_ticket_signature(&self, ticket: &str, nonce: &str, timestamp: u64, url: &str) -> String {
        signature::sha1(&format!(
            "jsapi_ticket={}&noncestr={}&timestamp={}&url={}",
            ticket, nonce, timestamp, url
        ))
    }

    /// 网页应用签名配置，对齐 createConfigSignature
    pub async fn create_config_signature(
        &self,
        url: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<Value> {
        let nonce = nonce.map(|v| v.to_owned()).unwrap_or_else(|| random(16));
        let timestamp = timestamp.unwrap_or_else(wechat_sdk::current_timestamp);
        let ticket = self.get_ticket().await?;
        let sign = self.get_ticket_signature(&ticket, &nonce, timestamp, url);

        Ok(json!({
            "appId": self.corp_id,
            "corpId": self.corp_id,
            "nonceStr": nonce,
            "timestamp": timestamp,
            "url": url,
            "signature": sign,
        }))
    }

    /// 应用签名配置，对齐 createAgentConfigSignature
    pub async fn create_agent_config_signature(
        &self,
        agent_id: i64,
        url: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<Value> {
        let nonce = nonce.map(|v| v.to_owned()).unwrap_or_else(|| random(16));
        let timestamp = timestamp.unwrap_or_else(wechat_sdk::current_timestamp);
        let ticket = self.get_agent_ticket(agent_id).await?;
        let sign = self.get_ticket_signature(&ticket, &nonce, timestamp, url);

        Ok(json!({
            "corpid": self.corp_id,
            "corpId": self.corp_id,
            "agentid": agent_id,
            "agentId": agent_id,
            "nonceStr": nonce,
            "timestamp": timestamp,
            "url": url,
            "signature": sign,
        }))
    }

    /// 读取缓存
    pub fn get_cached(key: &str) -> Option<String> {
        cache::get_raw(key)
    }

    /// 写入缓存
    pub fn set_cached(key: &str, ticket: &str) {
        cache::set_raw(key, ticket, 7200);
    }

    /// 清空缓存
    pub fn clear_cache() {
        cache::forget_prefix("work.jsapi_ticket");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        let ticket = JsApiTicket::new("corp", "secret");
        assert_eq!(ticket.get_key(), "work.jsapi_ticket.corp");
        assert_eq!(ticket.get_agent_key(7), "work.jsapi_ticket.corp.7");
    }

    #[test]
    fn signature_matches_formula() {
        let ticket = JsApiTicket::new("corp", "secret");
        let sign = ticket.get_ticket_signature("t", "n", 123, "https://a.b/c");
        let expect = wechat_sdk::signature::sha1("jsapi_ticket=t&noncestr=n&timestamp=123&url=https://a.b/c");
        assert_eq!(sign, expect);
    }
}
