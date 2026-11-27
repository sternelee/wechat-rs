//! copyright © ecdata.cn 2021 - present
//! 微信开放平台（第三方平台）接口，对齐 EasyWeChat\OpenPlatform
//!
//! 对应 EasyWeChat\OpenPlatform 模块：Account / Config / ComponentAccessToken /
//! VerifyTicket / AuthorizerAccessToken / Authorization / Encryptor / Server / Application

pub mod application;
pub mod authorization;
pub mod config;
pub mod server;
pub mod token;

pub use application::{Application, AUTHORIZE_URL};
pub use authorization::Authorization;
pub use config::{Account, OpenPlatformConfig};
pub use server::{HandlerKind, Server, DEFAULT_REPLY};
pub use token::{
    authorizer_cache_key, AuthorizerAccessToken, ComponentAccessToken, VerifyTicket,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn module_exports() {
        let conf = OpenPlatformConfig::from_value(&json!(
            {
                "app_id": "wxcomponent",
                "secret": "secret",
                "token": "token",
                "aes_key": "aes"
            }
        ))
        .unwrap();

        let app = Application::new(conf);
        assert_eq!(app.account().app_id, "wxcomponent");
        assert_eq!(DEFAULT_REPLY, "success");
    }
}
