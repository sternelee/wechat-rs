//! copyright © ecdata.cn 2021 - present
//! 企业微信接口对接，对齐 EasyWeChat\Work
//! created by shaipe 20210228
//!
//! 对应 EasyWeChat\Work 模块：Account / Config / AccessToken / JsApiTicket /
//! Encryptor / Server / Utils / Application / OAuth（getOAuth）

#[macro_use]
extern crate wechat_sdk;

pub mod application;
pub mod config;
pub mod encryptor;
pub mod oauth;
pub mod server;
pub mod ticket;
pub mod token;
pub mod utils;

pub use application::Application;
pub use config::{Account, WorkConfig};
pub use encryptor::Encryptor;
pub use oauth::{OAuth, OAUTH_AUTHORIZE_URL, QR_CONNECT_URL};
pub use server::Server;
pub use ticket::JsApiTicket;
pub use token::AccessToken;
pub use utils::Utils;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_exports() {
        let conf = crate::WorkConfig::new(crate::Account::new("corp", "secret"));
        let app = crate::Application::new(conf);
        assert_eq!(app.account().corp_id, "corp");
    }
}
