# Frozen comparison fixtures

[English](README.md) | [简体中文](README.zh-CN.md)

These JavaScript modules are test inputs for independent model, geometry and visual comparisons with the Rust application. Production code and packages do not import or install them.

`manifest.json` records source and fixture SHA-256 values. Desktop-surface construction is replaced with a throwing test stub. The reference gallery avoids constructing overlays in its temporary copy. Catalogs come from `core/locales` through the private test stage.

Keep the recorded fixture bytes unchanged. A deliberate behavior or visual-baseline update needs separate review; changing a fixture solely to make a comparison pass defeats the check. Parity tools compare against Rust or supply the reference UI used by integration tests.
