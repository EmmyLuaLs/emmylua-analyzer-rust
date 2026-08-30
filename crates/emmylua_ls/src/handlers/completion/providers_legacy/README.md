# 旧 completion providers —— 参考材料（未接入编译）

M4 迁移时这些 provider 被替换为 `../providers.rs`（salsa 版）。
此目录保留 git 原样的旧实现，仅作参考资料，逐步迁移到 salsa 架构后再删除。

原路径：`src/handlers/completion/providers/`。
之所以改名 `providers_legacy`：Rust 不允许 `providers.rs` 与 `providers/mod.rs` 同时被 `mod providers;` 解析（E0761）。
