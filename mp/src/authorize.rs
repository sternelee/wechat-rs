//! copyright © ecdata.cn 2021 - present
//! 微信公众号授权
use serde_json::Value;
use wechat_redis::{RedisStorage, SessionStore};
use wechat_sdk::{WechatResult, Client};
use wechat_sdk::constant::API_DOMAIN;

const WECHAT_OPEN_URI: &'static str = "https://open.weixin.qq.com";
///网页授权
pub struct WechatAuthorize {
    app_id: String,
    com_app_id: String,
}
impl WechatAuthorize {
    ///
    pub fn new(_app_id: &str, _com_app_id: &str) -> WechatAuthorize {
        WechatAuthorize {
            app_id: _app_id.to_string(),
            com_app_id: _com_app_id.to_string(),
        }
    }
   
    /// 授权页面
    pub fn get_authorize_url(
        &self,
        redirect_uri: &str,
        state: &str,
        scope: &Vec<&str>,
        response_type: &str,
    ) -> String {
        use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

        let encode_uri = if redirect_uri.starts_with("http") {
            utf8_percent_encode(redirect_uri, NON_ALPHANUMERIC).to_string()
        } else {
            utf8_percent_encode(&format!("http://{}", redirect_uri), NON_ALPHANUMERIC).to_string()
        };

        let uri=format!("{}{}",WECHAT_OPEN_URI,format!("/connect/oauth2/authorize?appid={}&redirect_uri={}&response_type={}&scope={}&state={}&component_appid={}#wechat_redirect",
        self.app_id,encode_uri,response_type,scope.join(","),state,self.com_app_id));

        uri
    }
    /// 代公众号网页授权的换 token 地址
    ///
    /// 注意：该接口在 `api.weixin.qq.com`，只有授权页 `/connect/oauth2/authorize`
    /// 在 `open.weixin.qq.com`（对齐 EasyWeChat OfficialAccount/OAuth 的 component 分支）。
    pub fn component_access_token_url(&self, code: &str, comp_access_token: &str) -> String {
        format!(
            "{domain}/sns/oauth2/component/access_token?appid={appid}&code={code}&grant_type={grant_type}&component_appid={component_appid}&component_access_token={component_access_token}",
            domain = API_DOMAIN,
            appid = &self.app_id,
            code = code,
            grant_type = "authorization_code",
            component_appid = &self.com_app_id,
            component_access_token = comp_access_token
        )
    }

    /// 获取用户access_token
    pub async fn fetch_user_access_token(&self, code: &str, comp_access_token: &str) -> WechatResult<Value> {
        let url = self.component_access_token_url(code, comp_access_token);

        let api = Client::new();
        let res = api.get(&url).await?;
        let data = wechat_sdk::json_decode(&res)?;
        Ok(data)
    }

    /// 用户信息地址（对齐 OfficialAccount/OAuth，必须走 api.weixin.qq.com）
    pub fn user_info_url(&self, open_id: &str, access_token: &str) -> String {
        format!(
            "{}/cgi-bin/user/info?access_token={}&openid={}&lang=zh_CN",
            API_DOMAIN, access_token, open_id
        )
    }
    /// 获取用户基本信息
    pub async fn fetch_user_info(&self, open_id: &str, access_token: &str) -> WechatResult<Value> {
        let url = self.user_info_url(open_id, access_token);

        let api = Client::new();
        let res = api.get(&url).await?;
        let data = wechat_sdk::json_decode(&res)?;
        Ok(data)
    }
}

const AUTHORIZE_CATCHE_KEY: &str = "AUTHORIZE_CATCHE_KEY";

/// 设置单个
pub fn set_authorize_token(redis_con: &str, key: &str, cnf: (String, i64)) {
    let url = format!("{}", redis_con);
    let cache_key = format!("{0}_{1}", AUTHORIZE_CATCHE_KEY, key);
    match RedisStorage::from_url(url) {
        Ok(session) => {
            session.set(cache_key, format!("{}_{}", cnf.0, cnf.1), Some(2 * 55 * 60));
        }
        Err(e) => {
            log!("save authorize token error {:?}", e);
        }
    }
}
/// 获取
pub fn get_authorize_token(redis_con: &str, key: &str) -> WechatResult<(String, i64)> {
    let cache_key = format!("{0}_{1}", AUTHORIZE_CATCHE_KEY, key);
    match RedisStorage::from_url(format!("{}", redis_con)) {
        Ok(session) => {
            let d = "".to_owned();
            if let Some(v) = session.get(cache_key, "get".to_owned(), Some(d)) {
                let arr: Vec<_> = v.split('_').collect();
                if arr.len() == 2 {
                    return Ok((arr[0].to_string(), arr[1].parse::<i64>().unwrap()));
                }
                Err(error! {code:600,msg:"数据不准确"})
            } else {
                Err(error! {code:600,msg:"数据不准确"})
            }
        }
        Err(e) => {
            let msg = format!("{:?}", e);
            Err(error! {code:600,msg:msg})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authorize() -> WechatAuthorize {
        WechatAuthorize::new("wx_app", "wx_component")
    }

    #[test]
    fn authorize_page_uses_open_domain() {
        let url = authorize().get_authorize_url(
            "https://example.com/cb",
            "state1",
            &vec!["snsapi_userinfo"],
            "code",
        );

        assert!(url.starts_with("https://open.weixin.qq.com/connect/oauth2/authorize"));
        assert!(url.contains("component_appid=wx_component"));
    }

    #[test]
    fn component_token_exchange_uses_api_domain() {
        let url = authorize().component_access_token_url("code1", "comp-token");

        assert!(url.starts_with("https://api.weixin.qq.com/sns/oauth2/component/access_token"));
        assert!(url.contains("component_access_token=comp-token"));
        assert!(url.contains("appid=wx_app"));
    }

    #[test]
    fn user_info_uses_api_domain() {
        let url = authorize().user_info_url("openid1", "access-token");

        assert!(url.starts_with("https://api.weixin.qq.com/cgi-bin/user/info"));
        assert!(url.contains("lang=zh_CN"));
    }
}
