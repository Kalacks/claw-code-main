# Claw Code（Windows：CLI/GUI 编译与使用说明）

本文档仅保留 Windows 下最常用的操作：

- 如何在 CLI 模式配置模型
- 如何在 GUI 模式配置模型
- 如何从源码编译 `cli.exe` 和 `gui.exe`
- 如何启动 `cli.exe` 和 `gui.exe`

## 1. 进入 Rust 工作区

```powershell
Set-Location E:\360Downloads\claw-code-main\rust
```

如果你的实际目录不同，请替换成你自己的路径。

## 2. 模型配置

### 2.1 CLI 模式配置模型

#### 临时指定（单次生效）

```powershell
# 交互模式
.\target\release\claw.exe --model deepseek-chat

# 单次 prompt
.\target\release\claw.exe --model deepseek-chat prompt "你好"
```

#### 持久化指定（默认模型）

在仓库根目录创建或修改 `.claw\settings.json`，示例：

```json
{
  "model": "deepseek-chat",
  "aliases": {
    "fast": "deepseek-chat",
    "smart": "claude-sonnet-4-6"
  }
}
```

然后可以这样用：

```powershell
.\target\release\claw.exe --model fast
```

### 2.2 GUI 模式配置模型

1. 启动 `claw-gui.exe`
2. 切到 `Models` 页面
3. 填写：`Name`、`Provider`、`Model`、`Base URL`、`API Key`（或环境变量模式）
4. 点击 `Save + Activate`
5. 返回聊天页后即使用该激活模型

## 3. 从源码编译 cli.exe 和 gui.exe

> 下面命令会同时编译 CLI 与 GUI（Release）。

```powershell
# 1) 进入 Rust 工作区
Set-Location E:\360Downloads\claw-code-main\rust

# 2) 创建输出目录（你指定的目录）
New-Item -ItemType Directory -Path E:\test3 -Force | Out-Null

# 3) 构建完整 GUI + CLI（Release）
cargo build -p rusty-claude-cli --release --features gui --bin claw --bin claw-gui

# 4) 复制并改名为你需要的文件名
Copy-Item .\target\release\claw.exe E:\test3\cli.exe -Force
Copy-Item .\target\release\claw-gui.exe E:\test3\gui.exe -Force

# 5) 校验输出
Get-ChildItem E:\test3\*.exe
```

## 4. 启动 cli.exe 和 gui.exe

### 4.1 启动 CLI

```powershell
# 交互模式
E:\test3\cli.exe

# 指定模型并单次执行
E:\test3\cli.exe --model deepseek-chat prompt "请总结当前项目"
```

### 4.2 启动 GUI

```powershell
# 方式 1：直接启动
E:\test3\gui.exe

# 方式 2：后台启动（PowerShell）
Start-Process E:\test3\gui.exe
```

