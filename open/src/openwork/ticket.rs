//! copyright © ecdata.cn 2021 - present
//! 授权企业 jsapi_ticket，对齐 EasyWeChat\OpenWork\JsApiTicket

use crate::openwork::token::AuthorizerAccessToken;
use serde_json::{json, Value};
use wechat_sdk::{
    cache, constant::WORK_API_DOMAIN, signature, strutil::random, Client, WechatResult as Result,
};

/// ticket 默认有效期（微信返回的 expires_in 通常为 7200）
const DEFAULT_TICKET_TTL: u64 = 7200;

fn cache_get(key: &str) -> Option<String> {
    cache::get_raw(key)
}

fn cache_set(key: &str, value: &str, ttl: u64) {
    cache::set_raw(key, value, ttl);
}

/// 取响应里的 expires_in，缺省用 7200，对齐 PHP `$this->cache->set($key, $ticket, $response['expires_in'])`
fn response_ttl(res: &Value) -> u64 {
    res["expires_in"]
        .as_u64()
        .filter(|ttl| *ttl > 0)
        .unwrap_or(DEFAULT_TICKET_TTL)
}

/// 授权企业 jsapi_ticket，对齐 OpenWork\JsApiTicket
#[derive(Debug, Clone)]
pub struct JsApiTicket {
    corp_id: String,
    access_token: Option<AuthorizerAccessToken>,
    key: Option<String>,
    base_domain: String,
    client: Client,
}

impl JsApiTicket {
    /// 创建 ticket 管理器
    pub fn new<S: Into<String>>(corp_id: S, access_token: Option<AuthorizerAccessToken>) -> JsApiTicket {
        JsApiTicket {
            corp_id: corp_id.into(),
            access_token,
            key: None,
            base_domain: WORK_API_DOMAIN.to_owned(),
            client: Client::default(),
        }
    }

    /// 覆盖 API 域名（测试或代理场景）
    pub fn with_base_domain<S: Into<String>>(mut self, domain: S) -> Self {
        self.base_domain = domain.into();
        self
    }

    /// 缓存 key，对齐 getKey
    pub fn get_key(&self) -> String {
        match &self.key {
            Some(key) => key.clone(),
            None => format!("open_work.jsapi_ticket.{}", self.corp_id),
        }
    }

    /// 应用级缓存 key，对齐 getAgentKey
    pub fn get_agent_key(&self, agent_id: i64) -> String {
        format!("{}.{}", self.get_key(), agent_id)
    }

    /// 自定义缓存 key
    pub fn set_key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    /// 获取 jsapi_ticket，对齐 getTicket
    pub async fn get_ticket(&self) -> Result<String> {
        let key = self.get_key();
        if let Some(ticket) = cache_get(&key) {
            return Ok(ticket);
        }

        let access_token = self.access_token_value().await?;
        let url = Client::build_url(
            &self.base_domain,
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

        cache_set(&key, ticket, response_ttl(&res));

        Ok(ticket.to_owned())
    }

    /// 获取应用级 ticket（agent_config），对齐 getAgentTicket
    pub async fn get_agent_ticket(&self, agent_id: i64) -> Result<String> {
        let key = self.get_agent_key(agent_id);
        if let Some(ticket) = cache_get(&key) {
            return Ok(ticket);
        }

        let access_token = self.access_token_value().await?;
        let url = Client::build_url(
            &self.base_domain,
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

        cache_set(&key, ticket, response_ttl(&res));

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
        nonce: &str,
        timestamp: u64,
        url: &str,
        js_api_list: &[&str],
        debug: bool,
        beta: bool,
    ) -> Result<Value> {
        let ticket = self.get_ticket().await?;

        Ok(json!({
            "appId": self.corp_id,
            "corpId": self.corp_id,
            "nonceStr": nonce,
            "timestamp": timestamp,
            "url": url,
            "signature": self.get_ticket_signature(&ticket, nonce, timestamp, url),
            "jsApiList": js_api_list,
            "debug": debug,
            "beta": beta,
        }))
    }

    /// 应用签名配置，对齐 createAgentConfigSignature
    pub async fn create_agent_config_signature(
        &self,
        agent_id: i64,
        nonce: &str,
        timestamp: u64,
        url: &str,
        js_api_list: &[&str],
    ) -> Result<Value> {
        let ticket = self.get_agent_ticket(agent_id).await?;

        Ok(json!({
            "corpid": self.corp_id,
            "corpId": self.corp_id,
            "agentid": agent_id,
            "agentId": agent_id,
            "nonceStr": nonce,
            "timestamp": timestamp,
            "url": url,
            "signature": self.get_ticket_signature(&ticket, nonce, timestamp, url),
            "jsApiList": js_api_list,
        }))
    }

    /// 生成随机 nonce
    pub fn random_nonce() -> String {
        random(16)
    }

    async fn access_token_value(&self) -> Result<String> {
        match &self.access_token {
            Some(token) => token.get_token().await,
            None => Err(error! {code: -1, msg: "No access_token available for JsApiTicket."}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        let ticket = JsApiTicket::new("corp", None);
        assert_eq!(ticket.get_key(), "open_work.jsapi_ticket.corp");
        assert_eq!(ticket.get_agent_key(9), "open_work.jsapi_ticket.corp.9");
    }

    #[test]
    fn response_ttl_prefers_expires_in() {
        assert_eq!(response_ttl(&json!({"expires_in": 5400})), 5400);
        assert_eq!(response_ttl(&json!({"expires_in": 0})), DEFAULT_TICKET_TTL);
        assert_eq!(response_ttl(&json!({})), DEFAULT_TICKET_TTL);
    }

    #[tokio::test]
    async fn ticket_is_cached_and_shared() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let (port, _) = serve_ticket(counter);

        let ticket = JsApiTicket::new("corp-cache-test", Some(seeded_token()))
            .with_base_domain(format!("http://127.0.0.1:{}", port));
        cache::forget(&ticket.get_key());

        assert_eq!(ticket.get_ticket().await.unwrap(), "TICKET-1");
        // 第二次走缓存，不再发请求（对齐 PHP cache->get / set）
        assert_eq!(ticket.get_ticket().await.unwrap(), "TICKET-1");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(
            cache::get_raw(&ticket.get_key()).as_deref(),
            Some("TICKET-1"),
            "ticket 必须写入共享缓存"
        );

        cache::forget(&ticket.get_key());
    }

    fn seeded_token() -> AuthorizerAccessToken {
        AuthorizerAccessToken::new("corp-cache-test", "SEEDED-TOKEN", None)
    }

    fn serve_ticket(hits: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> (u16, ()) {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(_) => continue,
                };

                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                let body = "{\"ticket\":\"TICKET-1\",\"expires_in\":7200}";
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
        });

        (port, ())
    }

    #[test]
    fn signature_formula() {
        let ticket = JsApiTicket::new("corp", None);
        assert_eq!(
            ticket.get_ticket_signature("tk", "nc", 100, "https://a.b"),
            signature::sha1("jsapi_ticket=tk&noncestr=nc&timestamp=100&url=https://a.b")
        );
    }
}
