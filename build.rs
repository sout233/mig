fn main() {
    println!("cargo:rerun-if-changed=assets/mig.rc");
    println!("cargo:rerun-if-changed=assets/icons/mig_icon_512.ico");

    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for("assets/mig.rc", ["mig"], embed_resource::NONE)
            .manifest_required()
            .expect("无法将 MIG 图标嵌入 Windows 可执行文件");
    }
}
