//! copyright © ecdata.cn 2021 - present
//! 扫码支付 URL Scheme 构建，对齐 EasyWeChat\Pay\URLSchemeBuilder

use crate::merchant::Merchant;
use crate::signature::LegacySignature;
use std::collections::BTreeMap;
use wechat_sdk::strutil::random;
use wechat_sdk::WechatResult as Result;

/// URL Scheme 构建器
#[derive(Debug, Clone)]
pub struct URLSchemeBuilder {
    merchant: Merchant,
}

impl URLSchemeBuilder {
    pub fn new(merchant: Merchant) -> URLSchemeBuilder {
        URLSchemeBuilder { merchant }
    }

    /// 商品二维码链接，对齐 forProduct
    pub fn for_product(&self, product_id: &str, app_id: &str) -> Result<String> {
        let mut params: BTreeMap<String, String> = BTreeMap::new();
        params.insert("appid".to_owned(), app_id.to_owned());
        params.insert("mch_id".to_owned(), self.merchant.mch_id.clone());
        params.insert(
            "time_stamp".to_owned(),
            wechat_sdk::current_timestamp().to_string(),
        );
        params.insert("nonce_str".to_owned(), random(16));
        params.insert("product_id".to_owned(), product_id.to_owned());

        let signed = LegacySignature::new(&self.merchant).sign(&params)?;
        let query = signed
            .iter()
            .map(|(key, value)| format!("{}={}", key, percent_encode(value)))
            .collect::<Vec<String>>()
            .join("&");

        Ok(format!("weixin://wxpay/bizpayurl?{}", query))
    }

    /// 已有 code_url 转 scheme，对齐 forCodeUrl
    pub fn for_code_url(&self, code_url: &str) -> String {
        format!("weixin://wxpay/bizpayurl?sr={}", code_url)
    }
}

fn percent_encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merchant() -> Merchant {
        Merchant::new("1900000109", "key", "cert", "v3", Some("chengduhongtuikeji20210911888888".into()))
    }

    #[test]
    fn build_product_scheme() {
        let builder = URLSchemeBuilder::new(merchant());
        let scheme = builder.for_product("PRODUCT1", "wx-app").unwrap();

        assert!(scheme.starts_with("weixin://wxpay/bizpayurl?"));
        assert!(scheme.contains("product_id=PRODUCT1"));
        assert!(scheme.contains("sign="));
    }

    #[test]
    fn build_code_url_scheme() {
        let builder = URLSchemeBuilder::new(merchant());
        assert_eq!(
            builder.for_code_url("weixin://wxpay/bizpayurl?pr=abc"),
            "weixin://wxpay/bizpayurl?sr=weixin://wxpay/bizpayurl?pr=abc"
        );
    }
}
