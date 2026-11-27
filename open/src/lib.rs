//! copyright
//! 微信第三方平台

#[macro_use]
extern crate wechat_sdk;

// 此句一定不能少
use wechat_sdk::WechatResult;

#[macro_use]
extern crate serde_json;

#[macro_use]
extern crate lazy_static;

mod ticket;
pub use ticket::Ticket;

mod config;
pub use config::{get_tripartite_config, get_tripartite_config_mut, set_tripartite_config, Config};

mod token;
pub use token::AuthToken;

mod open;
pub use open::OpenAccount;

pub mod weapp;
pub mod openplatform;
pub mod openwork;

// 定义接口请求域名
pub(crate) const API_DOMAIN: &'static str = "https://api.weixin.qq.com";

/// access_token 失效的错误码，对齐 EasyWeChat AccessTokenExpiredRetryStrategy
pub const TOKEN_EXPIRED_ERRCODES: [i32; 3] = [40001, 40014, 42001];

/// 判断错误是否为 access_token 失效（可用来自行决定是否刷新 token 后重试）
pub fn is_token_expired(err: &wechat_sdk::WechatError) -> bool {
    match &err.kind {
        wechat_sdk::ErrorKind::Custom { code, .. } => TOKEN_EXPIRED_ERRCODES.contains(code),
        _ => false,
    }
}

/// 解析 post 请求结果
pub(crate) async fn parse_json(res: &str) -> WechatResult<serde_json::Value> {
    wechat_sdk::json_decode(res)
}

#[cfg(test)]
mod tests {
    use crate::{get_tripartite_config, AuthToken, Config as TripartiteConfig};

    #[test]
    fn token_expired_error_codes() {
        let err = wechat_sdk::WechatError::custom(42001, "access_token expired");
        assert!(crate::is_token_expired(&err));

        let ok = wechat_sdk::WechatError::custom(-40001, "invalid signature");
        assert!(!crate::is_token_expired(&ok));
    }

    #[test]
    #[ignore = "需要真实第三方平台配置与外网，默认跳过"]
    fn test_redis() {
        let tripart_config: TripartiteConfig = get_tripartite_config();
        // let redis_config: RedisConfig = get_redis_conf();
        let comp = AuthToken::new(tripart_config.clone());
        let x =
            tokio::runtime::Runtime::new().unwrap().block_on(comp.fetch_authorizer_info("wx999317f16de96ce3", ""));
        let _ = x;
    }
}
