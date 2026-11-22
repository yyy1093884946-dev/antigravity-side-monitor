# Antigravity Side Monitor (`AntigravityMonitor`)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform: Windows](https://img.shields.io/badge/Platform-Windows%20x64-0078D6.svg)](#)
[![Language: Rust](https://img.shields.io/badge/Rust-Pure%20std%20%2B%20Win32-orange.svg)](#)

> An ultra-lightweight (~7 MB RAM, ~0% idle CPU, ~228 KB binary), **100% read-only** native Rust Win32 HUD capsule docked inside Google Antigravity's top title bar, showing real-time **Context Usage**, **TPS (Tokens/sec)**, and **Cache Hit Rate** without touching `app.asar`, injecting processes, or making any network/RPC calls.

一个极致轻量、**100% 合规零封号风险**的 Windows 原生监控胶囊，直接内嵌停靠在官方原版 **Google Antigravity** 客户端顶栏右侧（`— □ ×` 左侧），提供接近原生集成的视觉体验，同时完全规避修改客户端带来的 ToS 封号风险。

---

## 为什么选择纯外部只读方案（零封号风险）

- **不修改任何客户端文件**：不解包、不改写 `app.asar`，不修补任何二进制文件，客户端版本升级永不冲突。
- **不注入进程 / 不抢焦点**：纯独立 Win32 分层窗口（`WS_EX_LAYERED | WS_EX_NOACTIVATE`），不 Hook 宿主进程、不夺取输入框焦点。
- **不发任何网络或 RPC 请求**：不碰账号、不读 OAuth 凭据、不轮询任何内部接口。
- **纯只读增量扫描本地日志**：以只读方式增量读取 `%USERPROFILE%\.gemini\antigravity\brain\*\.system_generated\logs\transcript.jsonl` 尾部新增行，结合标准 Windows DWM / `SetWinEventHook` API 实现窗口内侧吸附跟随。

## 极致轻量化指标

- **单文件可执行程序**：仅约 `228 KB`（零外部 crate、零 .NET / WPF / WebView2 依赖）。
- **极低内存占用**：实测工作集内存约 **`~7 MB`**（相比同类 Electron/WPF 方案降低 90% 以上）。
- **闲时近零 CPU**：使用 `SetWinEventHook` 事件驱动窗口跟随（无高频坐标轮询），`transcript.jsonl` 无新增字节时不读取、不分配、不重绘。

## 功能与交互操作

顶栏胶囊默认实时展示：**上下文环形进度 + `CTX%` ｜ `TPS` ｜ `HIT%`**

| 操作方式 | 效果说明 |
| :--- | :--- |
| **左键点击胶囊** | 在顶栏下方原位展开 / 收起详细面板（当前模型、上下文进度条、本轮与会话平均 TPS、命中/未命中 Token 总量） |
| **按住胶囊拖动** | 自由微调在 Antigravity 窗口内的停靠位置（自动限制在客户端边界内） |
| **展开面板右上角 `[透明 / 同色]`** | 一键切换 **同色融底**（匹配 Antigravity `#16151B` 胶囊）与 **纯透明悬浮** |
| **展开面板 `[256K / 512K / 1M / 200K]`** | 循环切换上下文窗口参考上限 |
| **展开面板 `[复]`** | 一键复位回顶栏默认停靠位置 |
| **右键胶囊或系统托盘图标** | 切换透明背景、上下文上限、预设位置（顶栏默认 / 右上内侧 / 右下内侧）、始终置顶、打开日志目录或退出 |

---

## 快速使用

### 方式 1：直接下载运行（推荐）
前往本仓库右侧 [Releases](https://github.com/yyy1093884946-dev/antigravity-side-monitor/releases) 下载最新 `AntigravityMonitor.exe`，双击即可运行（内置单实例互斥，不会重复启动）。

### 方式 2：从源码编译
需要 Windows 上安装 [Rust](https://rustup.rs/) 工具链（MSVC 目标）：

```powershell
# 方式 A：使用自带脚本直接调用 rustc 编译（产出 ~228 KB 单文件）
powershell -NoProfile -ExecutionPolicy Bypass -File build.ps1

# 方式 B：使用 Cargo 编译
cargo build --release
```

### 运行自检（单元测试 + 本地日志解析验证）
```powershell
.\AntigravityMonitor.exe --self-test selftest.txt
Get-Content selftest.txt -Encoding UTF8
```

## 开源协议

本项目基于 [MIT License](LICENSE) 开源。
