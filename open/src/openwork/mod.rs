//! copyright © ecdata.cn 2021 - present
//! 企业微信服务商（开放平台）接口，对齐 EasyWeChat\OpenWork
//!
//! 对应 EasyWeChat\OpenWork 模块：Account / Config / ProviderAccessToken /
//! SuiteAccessToken / SuiteTicket / AuthorizerAccessToken / JsApiTicket /
//! Encryptor / SuiteEncryptor / Server / Authorization / Application

pub mod application;
pub mod config;
pub mod oauth;
pub mod server;
pub mod ticket;
pub mod token;

pub use application::{authorization_app_id, Application};
pub use config::{Account, OpenWorkConfig};
pub use oauth::{SuiteOAuth, SUITE_AUTHORIZE_URL, SUITE_QR_CONNECT_URL};
pub use server::{HandlerKind, Server, DEFAULT_REPLY};
pub use ticket::JsApiTicket;
pub use token::{AuthorizerAccessToken, ProviderAccessToken, SuiteAccessToken, SuiteTicket};

#[cfg(test)]
mod tests {
    #[test]
    fn module_exports() {
        let conf = crate::openwork::OpenWorkConfig::new(crate::openwork::Account::new(
            "corp", "ps", "suite", "ss", "token", "aes",
        ));
        let app = crate::openwork::Application::new(conf);
        assert_eq!(app.account().corp_id, "corp");
    }
}
