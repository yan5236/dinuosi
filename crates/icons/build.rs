use serde::Deserialize;
use std::{collections::HashSet, error::Error, fs, path::PathBuf};

#[derive(Deserialize)]
struct Spec {
    version: String,
    count: usize,
    icons: Vec<IconSpec>,
}

#[derive(Deserialize)]
struct IconSpec {
    name: String,
    label: String,
    file: String,
}

/// 读取图标清单，校验名称、数量和路径，并在 Cargo 输出目录生成枚举与嵌入资源。
/// 清单不合法、SVG 缺失或文件读写失败时终止构建；不改写清单或原始素材。
fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=spec.json");
    println!("cargo:rerun-if-changed=svg");
    let spec: Spec = serde_json::from_str(&fs::read_to_string("spec.json")?)?;
    assert_eq!(spec.count, spec.icons.len(), "图标数量与清单不一致");
    let mut names = HashSet::new();
    let mut variants = Vec::new();
    for icon in &spec.icons {
        assert!(
            icon.name
                .split('-')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_lowercase())),
            "图标名称必须是小写英文字母与连字符：{}",
            icon.name
        );
        assert!(names.insert(&icon.name), "图标名称重复：{}", icon.name);
        assert_eq!(icon.file, format!("svg/{}.svg", icon.name));
        fs::read(&icon.file)?;
        variants.push(
            icon.name
                .split('-')
                .map(|part| format!("{}{}", part[..1].to_ascii_uppercase(), &part[1..]))
                .collect::<String>(),
        );
    }
    assert_eq!(
        variants.iter().collect::<HashSet<_>>().len(),
        variants.len(),
        "生成的 Rust 图标名称重复"
    );
    let mut output = format!(
        "/// 素材清单的版本号。\npub const ICON_SET_VERSION: &str = {:?};\n\
         /// 由 spec.json 生成的轻线图标；不依赖任何 GUI 框架。\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\npub enum Icon {{\n",
        spec.version
    );
    for (icon, variant) in spec.icons.iter().zip(&variants) {
        output.push_str(&format!("    /// {}。\n    {},\n", icon.label, variant));
    }
    output.push_str("}\nimpl Icon {\n    /// 全部图标，顺序与素材清单一致。\n    pub const ALL: &'static [Self] = &[\n");
    for variant in &variants {
        output.push_str(&format!("        Self::{variant},\n"));
    }
    output.push_str("    ];\n");
    for (method, comment) in [
        (
            "name",
            "返回清单中的英文名称，供资源查询与稳定引用使用；不产生副作用。",
        ),
        (
            "label",
            "返回清单中的中文名称，供界面文字和无障碍描述使用；不产生副作用。",
        ),
        (
            "path",
            "返回嵌入资源的逻辑路径；与操作系统路径和运行目录无关。",
        ),
        (
            "svg",
            "返回原始 SVG 文本，保留 currentColor 和描边规格；无需磁盘访问。",
        ),
    ] {
        output.push_str(&format!(
            "    /// {comment}\n    pub fn {method}(self) -> &'static str {{\n        match self {{\n"
        ));
        for (icon, variant) in spec.icons.iter().zip(&variants) {
            let value = match method {
                "name" => format!("{:?}", icon.name),
                "label" => format!("{:?}", icon.label),
                "path" => format!("{:?}", format!("icons/{}.svg", icon.name)),
                _ => format!(
                    "include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{}\"))",
                    icon.file
                ),
            };
            output.push_str(&format!("            Self::{variant} => {value},\n"));
        }
        output.push_str("        }\n    }\n");
    }
    output.push_str("}\n");
    fs::write(
        PathBuf::from(std::env::var("OUT_DIR")?).join("icons.rs"),
        output,
    )?;
    Ok(())
}
