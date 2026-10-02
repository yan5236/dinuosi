use dinuosi_icons::Icon;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;
const LOGO_PATH: &str = "brand/dinuosi-primary.png";
const LOGO: &[u8] = include_bytes!("../../../assets/brand/png/dinuosi-primary-128.png");

/// 返回明确标记为内嵌资源的 Logo 图片源，避免普通字符串被 GPUI 当作网络 URI。
/// 只构造图片源描述，实际字节由 AssetSource 离线提供；不读写磁盘或发起网络请求。
pub fn logo_source() -> gpui::ImageSource {
    gpui::ImageSource::Resource(gpui::Resource::Embedded(LOGO_PATH.into()))
}

impl AssetSource for Assets {
    /// 按逻辑路径读取嵌入的图标或品牌 Logo；未知路径返回 None，不访问文件系统。
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let bytes = if path == LOGO_PATH {
            Some(LOGO)
        } else {
            Icon::ALL
                .iter()
                .find(|icon| icon.path() == path)
                .map(|icon| icon.svg().as_bytes())
        };
        Ok(bytes.map(Cow::Borrowed))
    }

    /// 列出指定逻辑目录下的嵌入资源；空路径列出全部，按路径分隔符匹配目录。
    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{}/", path.trim_end_matches('/'))
        };
        Ok(Icon::ALL
            .iter()
            .map(|icon| icon.path())
            .chain([LOGO_PATH])
            .filter(|asset| asset.starts_with(&prefix))
            .map(SharedString::from)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 GPUI 资源源可离线读取 Logo 和图标，并正确处理未知路径及目录前缀。
    #[test]
    fn asset_source_serves_embedded_resources_without_external_paths() {
        assert!(
            Assets
                .load(LOGO_PATH)
                .unwrap()
                .unwrap()
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );
        for icon in Icon::ALL {
            assert_eq!(
                Assets.load(icon.path()).unwrap().unwrap().as_ref(),
                icon.svg().as_bytes()
            );
        }
        assert!(
            matches!(logo_source(), gpui::ImageSource::Resource(gpui::Resource::Embedded(path)) if path.as_ref() == LOGO_PATH)
        );
        assert!(Assets.load("../outside.svg").unwrap().is_none());
        assert_eq!(Assets.list("").unwrap().len(), 90);
        assert_eq!(Assets.list("icons/").unwrap().len(), 89);
        assert_eq!(Assets.list("brand").unwrap().len(), 1);
        assert!(Assets.list("ico").unwrap().is_empty());
    }
}
