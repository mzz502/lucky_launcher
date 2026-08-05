fn main() {
    // 把 app.ico 以资源 1 嵌入 exe（窗口图标 + 托盘图标复用）。
    // 显式声明依赖，重新生成 app.ico（scripts/rebuild_icon.ps1）后才会触发重编译。
    println!("cargo:rerun-if-changed=app.ico");
    println!("cargo:rerun-if-changed=app.rc");
    // 3.x 返回 CompilationResult（#[must_use]），需显式处理。
    embed_resource::compile("app.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
