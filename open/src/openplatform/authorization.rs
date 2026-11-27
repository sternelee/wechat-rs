//! copyright © ecdata.cn 2021 - present
//! 授权信息，对齐 EasyWeChat\OpenPlatform\Authorization

use serde_json::Value;
use wechat_sdk::WechatResult as Result;

use super::token::AuthorizerAccessToken;

/// 授权信息
#[derive(Debug, Clone, Default)]
pub struct Authorization {
    data: Value,
}

impl Authorization {
    /// 创建授权信息
    pub fn new(data: Value) -> Authorization {
        Authorization { data }
    }

    /// 原始数据
    pub fn attributes(&self) -> &Value {
        &self.data
    }

    /// 授权方 appid
    pub fn get_app_id(&self) -> Option<&str> {
        self.data
            .get("authorization_info")
            .and_then(|info| info.get("authorizer_appid"))
            .and_then(Value::as_str)
    }

    /// 授权方 access_token
    pub fn get_access_token(&self) -> Result<AuthorizerAccessToken> {
        let info = self
            .data
            .get("authorization_info")
            .ok_or_else(|| missing("authorization_info"))?;
        let app_id = info
            .get("authorizer_appid")
            .and_then(Value::as_str)
            .ok_or_else(|| missing("authorizer_appid"))?;
        let token = info
            .get("authorizer_access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| missing("authorizer_access_token"))?;

        Ok(AuthorizerAccessToken::new(app_id, token))
    }

    /// 刷新令牌
    pub fn get_refresh_token(&self) -> Option<&str> {
        self.data
            .get("authorization_info")
            .and_then(|info| info.get("authorizer_refresh_token"))
            .and_then(Value::as_str)
    }

    /// 有效期
    pub fn get_expires_in(&self) -> u64 {
        self.data
            .get("authorization_info")
            .and_then(|info| info.get("expires_in"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    }
}

fn missing(key: &str) -> wechat_sdk::WechatError {
    error! {
        code: -1,
        msg: format!("Failed to get {}: missing key.", key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn authorization() -> Authorization {
        let info = json!(
            {
                "authorizer_appid": "wxauth",
                "authorizer_access_token": "token",
                "authorizer_refresh_token": "refresh",
                "expires_in": 7200
            }
        );

        Authorization::new(json!( { "authorization_info": info } ))
    }

    #[test]
    fn reads_fields() {
        let info = authorization();
        assert_eq!(info.get_app_id(), Some("wxauth"));
        assert_eq!(info.get_access_token().unwrap().get_token(), "token");
        assert_eq!(info.get_refresh_token(), Some("refresh"));
        assert_eq!(info.get_expires_in(), 7200);
    }

    #[test]
    fn missing_info_reports_error() {
        let info = Authorization::new(json!( {} ));
        assert!(info.get_access_token().is_err());
        assert!(info.get_app_id().is_none());
    }
}
