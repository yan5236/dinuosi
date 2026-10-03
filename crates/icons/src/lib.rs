//! 轻线 SVG 的类型化入口；由素材清单生成，原始 SVG 编译进程序。

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 验证全部嵌入图标保留清单名称、中文标签和统一 SVG 规格，避免资源接入遗漏。
    #[test]
    fn embedded_icons_preserve_the_supplied_asset_contract() {
        assert_eq!(ICON_SET_VERSION, "1.7");
        assert_eq!(Icon::ALL.len(), 102);
        let mut paths = HashSet::new();
        for icon in Icon::ALL {
            assert!(paths.insert(icon.path()));
            assert_eq!(icon.path(), format!("icons/{}.svg", icon.name()));
            assert!(!icon.label().is_empty());
            if *icon == Icon::PlayheadHead {
                assert!(icon.svg().contains("fill=\"currentColor\""));
                assert!(icon.svg().contains("viewBox=\"0 0 24 24\""));
                continue;
            }
            for attribute in [
                "viewBox=\"0 0 24 24\"",
                "stroke=\"currentColor\"",
                "stroke-width=\"1.8\"",
                "fill=\"none\"",
                "stroke-linecap=\"round\"",
                "stroke-linejoin=\"round\"",
            ] {
                assert!(
                    icon.svg().contains(attribute),
                    "{} 缺少 {}",
                    icon.name(),
                    attribute
                );
            }
        }
        assert_eq!(Icon::ReplaceMedia.label(), "替换素材");
    }
}
