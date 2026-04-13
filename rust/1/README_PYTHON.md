# Python Hello World 程序

本目录包含多个Python的"Hello, World!"程序示例，展示了不同的实现方式。

## 文件说明

### 1. `hello.py`
一个功能更丰富的Hello World程序，包含：
- 基本的"Hello, World!"输出
- 用户输入功能
- 显示Python版本信息
- 显示当前时间

### 2. `hello_simple.py`
最简单的Hello World程序，只有一行代码。

### 3. `hello_classic.py`
经典的Hello World程序，使用函数封装：
- 函数定义和调用
- 程序信息显示
- 格式化输出

### 4. `hello_multiple.py`
展示10种不同的Python Hello World写法：
1. 基本写法
2. 使用变量
3. 使用函数
4. 使用类
5. f-string格式化
6. format方法
7. join方法
8. 多行字符串
9. 带时间戳
10. 模拟文件读取

## 如何运行

### 方法1：使用Python命令
```bash
# 运行完整版
python hello.py

# 运行简单版
python hello_simple.py

# 运行经典版
python hello_classic.py

# 运行多种写法版
python hello_multiple.py
```

### 方法2：直接双击运行（Windows）
1. 确保已安装Python
2. 双击任何.py文件运行

### 方法3：使用Python交互模式
```bash
# 进入Python交互模式
python

# 然后输入
>>> print("Hello, World!")
```

## 程序功能

### hello.py 程序流程：
1. 打印"Hello, World!"
2. 提示用户输入名字
3. 个性化问候
4. 显示Python版本信息
5. 显示当前时间

### hello_simple.py 程序流程：
1. 打印"Hello, World!"

### hello_classic.py 程序流程：
1. 定义hello_world()函数
2. 调用函数并打印结果
3. 显示分隔线和程序信息
4. 输出程序元数据

### hello_multiple.py 程序流程：
1. 展示10种不同的Hello World写法
2. 每种方法都有说明
3. 包含用户交互功能
4. 显示总结信息

## 系统要求
- Python 3.x 或更高版本
- 任何操作系统（Windows、macOS、Linux）

## 学习要点

通过这4个Python Hello World程序，您可以学习到：

### 基础语法
- `print()`函数的使用
- 变量定义和赋值
- 注释的写法（单行`#`和多行`'''`）

### 函数和类
- 函数定义（`def`关键字）
- 函数调用和返回值
- 类的定义和使用（`class`关键字）
- 构造函数（`__init__`方法）

### 字符串处理
- 字符串连接
- f-string格式化（Python 3.6+）
- `format()`方法
- `join()`方法
- 多行字符串

### 程序结构
- `if __name__ == "__main__":`的作用
- 模块导入（`import`语句）
- 主函数模式

### 用户交互
- `input()`函数获取用户输入
- 条件判断（`if-else`语句）
- 字符串处理（`strip()`方法）

### 其他功能
- 时间日期处理（`datetime`模块）
- 系统信息获取（`sys`模块）
- 文件路径获取（`__file__`变量）