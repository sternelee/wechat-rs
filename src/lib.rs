//! copyright © shaipe 2020 - persent
//!

// #[allow(unused_imports)]
// #[macro_use]
// extern crate wechat_sdk;
//use wechat_sdk::WechatError;

#[cfg(feature = "open")]
pub mod open {
    pub use wechat_open::*;
}

/// 微信小程序
pub mod weapp {
    #[cfg(feature = "weapp")]
    pub use wechat_weapp::*;
    #[cfg(feature = "open")]
    pub use wechat_open::weapp::*;
}

/// 微信公众号
// #[cfg(feature = "mp")]
pub mod mp {
    // #[cfg(feature = "mp")]
    pub use wechat_mp::*;
}

/// 微信支付
#[cfg(feature = "pay")]
pub mod pay {
    pub use wechat_pay::*;
}

/// 微信小店
pub mod store {
    #[cfg(feature = "store")]
    #[allow(unused_imports)]
    pub use wechat_store::*;
}

/// 企业微信
pub mod work {
    #[cfg(feature = "work")]
    pub use wechat_work::*;
}

pub use wechat_sdk::*;

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn loads_config() {
        let conf = wechat_sdk::Config::load(json!({
            "app_id": "wx123",
            "secret": "secret",
            "mch_id": "1900000109",
            "secret_key": "v2-key",
        }))
        .unwrap();

        assert_eq!(conf.app_id, "wx123");
        assert_eq!(conf.mch_id, "1900000109");
    }

    #[test]
    fn exports_modules() {
        // 各模块的类型可通过根 crate 访问
        let _ = core::any::type_name::<crate::mp::MpConfig>();
        let _ = core::any::type_name::<crate::Client>();

        #[cfg(feature = "pay")]
        {
            let _ = core::any::type_name::<crate::pay::Merchant>();
        }

        #[cfg(feature = "work")]
        {
            let _ = core::any::type_name::<crate::work::WorkConfig>();
        }
    }
}
