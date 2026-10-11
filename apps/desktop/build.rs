/// 将现有品牌 ICO 编译进 Windows 可执行文件，供资源管理器和 GPUI 原生窗口共用。
/// 非 Windows 目标跳过；缺少资源编译器或图标编译失败时终止构建，避免生成默认图标的程序。
fn main() {
    println!("cargo:rerun-if-changed=resources/windows/app.rc");
    println!("cargo:rerun-if-changed=../../assets/brand/dinuosi.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("resources/windows/app.rc", embed_resource::NONE)
            .manifest_required()
            .expect("无法嵌入 Dinuosi Windows 图标");
    }
}
