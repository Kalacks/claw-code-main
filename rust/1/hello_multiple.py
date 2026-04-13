#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
Python Hello World 多种写法示例
展示Python中打印"Hello, World!"的不同方式
"""

# ==================== 方法1: 最基本的写法 ====================
print("方法1 - 基本写法:")
print("Hello, World!")

# ==================== 方法2: 使用变量 ====================
print("\n方法2 - 使用变量:")
message = "Hello, World!"
print(message)

# ==================== 方法3: 使用函数 ====================
print("\n方法3 - 使用函数:")
def say_hello():
    return "Hello, World!"

print(say_hello())

# ==================== 方法4: 使用类 ====================
print("\n方法4 - 使用类:")
class HelloWorld:
    def __init__(self):
        self.message = "Hello, World!"
    
    def display(self):
        return self.message

hw = HelloWorld()
print(hw.display())

# ==================== 方法5: 使用f-string格式化 ====================
print("\n方法5 - 使用f-string:")
language = "Python"
print(f"Hello, World! from {language}")

# ==================== 方法6: 使用format方法 ====================
print("\n方法6 - 使用format方法:")
template = "{} {}!"
print(template.format("Hello", "World"))

# ==================== 方法7: 使用join方法 ====================
print("\n方法7 - 使用join方法:")
words = ["Hello", "World"]
print(", ".join(words) + "!")

# ==================== 方法8: 多行字符串 ====================
print("\n方法8 - 多行字符串:")
multi_line = """
Hello,
World!
"""
print(multi_line.strip())

# ==================== 方法9: 带时间戳的Hello World ====================
print("\n方法9 - 带时间戳:")
from datetime import datetime
now = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
print(f"[{now}] Hello, World!")

# ==================== 方法10: 从文件读取 ====================
print("\n方法10 - 模拟从文件读取:")
# 在实际应用中可以从文件读取内容
file_content = "Hello, World!"
print(file_content)

# ==================== 总结 ====================
print("\n" + "="*50)
print("总结: 以上展示了10种不同的Python Hello World写法")
print("="*50)

# 用户交互版本
print("\n" + "="*50)
print("交互式Hello World")
print("="*50)

name = input("请输入您的名字: ")
if name.strip():
    print(f"\n你好, {name}! 欢迎学习Python编程！")
else:
    print("\n你好, 世界! 欢迎学习Python编程！")

print("\nPython Hello World 程序执行完毕！")