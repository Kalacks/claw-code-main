# 经典Python Hello World程序
# 这是学习任何编程语言的第一步

def hello_world():
    """打印Hello World的函数"""
    message = "Hello, World!"
    return message

# 主程序
if __name__ == "__main__":
    # 调用函数并打印结果
    result = hello_world()
    print(result)
    
    # 添加一些额外的输出
    print("=" * 30)
    print("恭喜！您成功运行了Python程序！")
    print("=" * 30)
    
    # 显示程序信息
    print(f"\n程序名称: {__file__}")
    print("作者: Python初学者")
    print("用途: 学习Python基础语法")