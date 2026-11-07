# Buttery

让 Windows 上的鼠标滚轮用起来像 macOS 一样顺。

## 能做什么

- **自然滚动**：将 MacOS 中的自然滚动特性带来 Windows
- **平滑滚动**：开启后常驻托盘，开机自动启动。
  - 全屏游戏时自动停用，游戏不受影响。
  - 按住 Ctrl 或 Alt 时不做平滑，维持 Windows 原本的滚动逻辑。

## 已知问题

- **资源管理器里平滑滚动不生效**
- 少数老程序在平滑模式下可能滚不动。
- 开启平滑滚动后别动 `buttery.exe` 的位置，否则自启会失效。

## 从源码构建

在 macOS 上交叉编译：

```sh
cargo install cargo-xwin
rustup target add x86_64-pc-windows-msvc
cargo xwin build --release
```

编译产物在 `target/x86_64-pc-windows-msvc/release/buttery.exe`。

平滑滚动的手感参数在 `src/smooth.rs` 开头，测试可以直接在 Mac 上开始：

```sh
rustc --edition 2021 --test src/smooth.rs -o /tmp/smooth_test && /tmp/smooth_test
```

## License

[GPL-3.0](LICENSE)
