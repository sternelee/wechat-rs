//! copyright © ecdata.cn 2021 - present
//! 微信小程序功能对接
//! created by shaipe 20210228

// mod weapp;
// pub use weapp::WeApp;

#[macro_use]
extern crate wechat_sdk;

// 引入json宏
#[macro_use]
extern crate serde_json;

mod auth;
pub use auth::Auth;

mod application;
pub use application::Application;
// 回调服务端（serve/handler/回复加密）复用 sdk::Server
pub use wechat_sdk::{Server as WeappServer, ServerResponse};

mod decryptor;
pub use decryptor::Decryptor;

mod utils;
pub use utils::Utils;

mod qrcode;
pub use qrcode::QRCode;

mod config;
pub use config::WeappConfig;

mod subscribe;
pub use subscribe::SubscribeMessage;

// mod domain;
// pub use domain::Domain;


// 接口域名
pub(crate) const API_DOMAIN: &'static str = "https://api.weixin.qq.com";
