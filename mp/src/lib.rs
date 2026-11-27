//! copyright © ecdata.cn 2021 - present
//! 公众号对接开发应用
//! created by shaipe 20210302

#[macro_use]
extern crate wechat_sdk;

#[macro_use]
extern crate serde_json;

pub mod config;
pub use config::{Account, MpConfig};

pub mod application;
pub use application::Application;

pub mod authorizer_oauth;
pub mod oauth;
pub use authorizer_oauth::AuthorizerOAuth;
pub use oauth::OAuth;

pub mod utils;
pub use utils::Utils;

pub mod server;
pub use server::MpServer;

pub mod message;

pub mod authorize;
pub use authorize::WechatAuthorize;
