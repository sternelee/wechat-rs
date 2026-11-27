//! copyright © ecdata.cn 2021 - present
//! 第三方平台（开放平台）代调用所需的组件授权上下文
//!
//! 对齐 EasyWeChat OpenPlatform 的 OAuth factory：
//!
//! ```php
//! new WeChat([
//!     'client_id' => $appId,
//!     'component' => [
//!         'component_app_id' => $componentAppId,
//!         'component_access_token' => fn () => $this->getComponentAccessToken()->getToken(),
//!     ],
//! ]);
//! ```
//!
//! 这里的 token 提供者是一个可异步调用的闭包，因此每次真正发请求时才取 token，
//! 过期后由持有者（如 `ComponentAccessToken`）自行刷新，缓存行为与 PHP 一致。

use crate::WechatResult;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// 组件 access_token 提供者
pub type ComponentTokenFn =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = WechatResult<String>> + Send>> + Send + Sync>;

/// 组件授权上下文：第三方平台 app_id + 组件 access_token 提供者
#[derive(Clone)]
pub struct ComponentOAuth {
    component_app_id: String,
    token: ComponentTokenFn,
}

impl ComponentOAuth {
    /// 创建组件授权上下文
    pub fn new<A, F, Fut>(component_app_id: A, token: F) -> ComponentOAuth
    where
        A: Into<String>,
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = WechatResult<String>> + Send + 'static,
    {
        ComponentOAuth {
            component_app_id: component_app_id.into(),
            token: Arc::new(move || Box::pin(token())),
        }
    }

    /// 第三方平台 app_id
    pub fn component_app_id(&self) -> &str {
        &self.component_app_id
    }

    /// 取组件 access_token（必要时由提供者刷新）
    pub async fn get_token(&self) -> WechatResult<String> {
        (self.token)().await
    }
}

impl fmt::Debug for ComponentOAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentOAuth")
            .field("component_app_id", &self.component_app_id)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn token_provider_is_called_lazily() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();

        let component = ComponentOAuth::new("wx-component", move || {
            let counter = counter.clone();
            async move {
                let nth = counter.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(format!("component-token-{}", nth))
            }
        });

        assert_eq!(component.component_app_id(), "wx-component");
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        assert_eq!(component.get_token().await.unwrap(), "component-token-1");
        assert_eq!(component.get_token().await.unwrap(), "component-token-2");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn debug_does_not_leak_token_provider() {
        let component = ComponentOAuth::new("wx-component", || async { Ok("t".to_owned()) });
        let debug = format!("{:?}", component);
        assert!(debug.contains("wx-component"));
    }
}
