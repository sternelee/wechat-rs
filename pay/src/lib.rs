//! copyright © ecdata.cn 2021 - present 
//! 微信支付模块
//! created by shaipe 20210914
//!
//! 对齐 EasyWeChat\Pay：Merchant / Signature(v3) / LegacySignature(v2) /
//! Validator / Server / Utils / Client / URLSchemeBuilder / Message / Application

#[macro_use]
extern crate wechat_sdk;

pub mod aes;
pub mod application;
pub mod client;
pub mod merchant;
pub mod message;
pub mod order;
pub mod server;
pub mod signature;
pub mod url_scheme;
pub mod utils;
pub mod validator;

pub use application::Application;
pub use client::{is_v3_path, Client, PAY_API_DOMAIN};
pub use merchant::Merchant;
pub use message::Message;
pub use order::Order;
pub use server::Server;
pub use signature::{LegacySignature, Signature};
pub use url_scheme::URLSchemeBuilder;
pub use utils::Utils;
pub use validator::Validator;

#[cfg(test)]
mod tests {
    #[test]
    fn public_api_is_reachable() {
        let merchant = crate::Merchant::new(
            "1900000109",
            "key",
            "cert",
            "v3",
            Some("chengduhongtuikeji20210911888888".into()),
        );

        let _ = crate::Utils::new(merchant.clone());
        let _ = crate::Client::new(merchant.clone());
        let _ = crate::Server::new(merchant.clone());
        let _ = crate::URLSchemeBuilder::new(merchant);
    }
}
