@echo off
chcp 65001 >nul
title Claw Code 启动器

:menu
cls
echo ========================================
echo            Claw Code 启动器
echo ========================================
echo.
echo 请选择要启动的版本：
echo.
echo 1. 命令行版本 (claw.exe) - 终端交互
echo 2. 图形界面版本 (claw-gui.exe) - GUI窗口
echo 3. 快速命令模式
echo 4. 系统检查和配置
echo 5. 显示帮助信息
echo 6. 退出
echo.
set /p choice="请输入选择 (1-6): "

if "%choice%"=="1" goto cli
if "%choice%"=="2" goto gui
if "%choice%"=="3" goto quick
if "%choice%"=="4" goto config
if "%choice%"=="5" goto help
if "%choice%"=="6" goto exit
echo 无效的选择，请按任意键重试...
pause >nul
goto menu

:cli
cls
echo ========================================
echo      Claw Code 命令行版本
echo ========================================
echo.
echo 选项：
echo 1. 交互式REPL模式
echo 2. 单次提示模式
echo 3. 查看帮助
echo 4. 返回主菜单
echo.
set /p cli_choice="请选择: "

if "%cli_choice%"=="1" (
    echo 启动交互式REPL模式...
    echo.
    claw.exe
    pause
    goto menu
)

if "%cli_choice%"=="2" (
    echo.
    set /p prompt="请输入要发送的提示: "
    if "%prompt%"=="" (
        echo 提示不能为空！
        pause
        goto cli
    )
    echo 发送提示: %prompt%
    echo.
    claw.exe prompt "%prompt%"
    pause
    goto menu
)

if "%cli_choice%"=="3" (
    echo.
    claw.exe --help
    pause
    goto cli
)

if "%cli_choice%"=="4" goto menu
echo 无效的选择！
pause
goto cli

:gui
cls
echo ========================================
echo     Claw Code 图形界面版本
echo ========================================
echo.
echo 正在启动图形界面...
echo 注意：GUI版本将在新窗口中打开
echo.
start claw-gui.exe
echo 按任意键返回主菜单...
pause >nul
goto menu

:quick
cls
echo ========================================
echo         Claw Code 快速命令
echo ========================================
echo.
echo 常用命令：
echo 1. 查看版本信息
echo 2. 运行健康检查
echo 3. 登录认证
echo 4. 查看状态
echo 5. 自定义命令
echo 6. 返回主菜单
echo.
set /p quick_choice="请选择: "

if "%quick_choice%"=="1" (
    echo.
    claw.exe version
    pause
    goto quick
)

if "%quick_choice%"=="2" (
    echo.
    claw.exe doctor
    pause
    goto quick
)

if "%quick_choice%"=="3" (
    echo.
    claw.exe login
    pause
    goto quick
)

if "%quick_choice%"=="4" (
    echo.
    claw.exe status
    pause
    goto quick
)

if "%quick_choice%"=="5" (
    echo.
    set /p custom_cmd="请输入命令 (如: prompt '你好'): "
    if "%custom_cmd%"=="" (
        echo 命令不能为空！
        pause
        goto quick
    )
    echo 执行: claw.exe %custom_cmd%
    echo.
    claw.exe %custom_cmd%
    pause
    goto quick
)

if "%quick_choice%"=="6" goto menu
echo 无效的选择！
pause
goto quick

:config
cls
echo ========================================
echo      Claw Code 系统配置
echo ========================================
echo.
echo 配置选项：
echo 1. 查看当前配置
echo 2. 初始化工作区
echo 3. 清理会话数据
echo 4. 查看磁盘使用情况
echo 5. 返回主菜单
echo.
set /p config_choice="请选择: "

if "%config_choice%"=="1" (
    echo.
    echo 配置文件位置: .claw\
    dir .claw /s
    echo.
    pause
    goto config
)

if "%config_choice%"=="2" (
    echo.
    echo 正在初始化工作区...
    claw.exe init
    pause
    goto config
)

if "%config_choice%"=="3" (
    echo.
    echo 警告：这将清理所有会话数据！
    set /p confirm="确认清理？(y/n): "
    if /i "%confirm%"=="y" (
        if exist .claw\sessions (
            rmdir /s /q .claw\sessions
            mkdir .claw\sessions
            echo 会话数据已清理。
        ) else (
            echo 会话目录不存在。
        )
    )
    pause
    goto config
)

if "%config_choice%"=="4" (
    echo.
    echo 当前目录大小:
    for /f "tokens=3" %%a in ('dir /s ^| find "个文件"') do echo 总大小: %%a
    echo.
    echo 可执行文件大小:
    dir *.exe
    echo.
    pause
    goto config
)

if "%config_choice%"=="5" goto menu
echo 无效的选择！
pause
goto config

:help
cls
type README.txt
echo.
echo 按任意键返回主菜单...
pause >nul
goto menu

:exit
echo 感谢使用 Claw Code！
timeout /t 2 >nul
exit