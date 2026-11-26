# 冻结的比较夹具

[English](README.md) | [简体中文](README.zh-CN.md)

这些 JavaScript 模块用于与 Rust 程序进行独立模型、几何和视觉比较。生产代码和发布包不会导入或安装它们。

`manifest.json` 记录来源和夹具的 SHA-256。桌面界面构造被替换为抛错测试桩；参考图库在临时副本中跳过 overlay 构造。翻译目录由私有测试暂存流程从 `core/locales` 提供。

保持 manifest 对应文件内容不变。主动修改行为或视觉基线需单独审阅，不得仅为通过比较而修改夹具。比较工具会与 Rust 核对，或为集成测试提供参考 UI。
