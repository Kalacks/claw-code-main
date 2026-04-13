# 工具模块优化方案

## 当前问题
1. 单文件过大（8760行）
2. 工具定义与实现耦合
3. 重复的JSON序列化
4. 字符串操作频繁

## 优化目标
- 减少token消耗（编译时和运行时）
- 提高代码可维护性
- 保持功能完整性

## 具体方案

### 第一阶段：模块拆分（立即执行）
将 `lib.rs` 拆分为以下模块：

```
src/
├── lib.rs                    # 主入口，重新导出模块
├── core/
│   ├── mod.rs               # 核心类型定义
│   ├── registry.rs          # 工具注册表
│   └── executor.rs          # 工具执行器
├── tools/
│   ├── mod.rs               # 工具模块入口
│   ├── file_ops.rs          # 文件操作工具
│   ├── web_ops.rs           # 网络操作工具
│   ├── task_ops.rs          # 任务管理工具
│   ├── worker_ops.rs        # Worker管理工具
│   ├── agent_ops.rs         # Agent相关工具
│   └── misc_ops.rs          # 其他工具
├── utils/
│   ├── mod.rs               # 工具函数
│   ├── json.rs              # JSON处理优化
│   ├── permission.rs        # 权限检查
│   └── search.rs            # 工具搜索
└── tests/
    └── mod.rs               # 测试模块
```

### 第二阶段：JSON处理优化
1. 使用 `serde_json::Value` 直接传递，避免字符串转换
2. 实现惰性序列化，只在需要时转换为字符串
3. 缓存常用工具的参数schema

### 第三阶段：字符串操作优化
1. 使用 `Cow<str>` 避免不必要的分配
2. 预编译正则表达式
3. 减少中间字符串创建

## 具体修改步骤

### 步骤1：创建模块结构
```bash
mkdir -p rust/crates/tools/src/{core,tools,utils}
```

### 步骤2：提取核心类型到 `core/mod.rs`
```rust
// 提取 ToolSpec, GlobalToolRegistry 等核心类型
pub struct ToolSpec { /* ... */ }
pub struct GlobalToolRegistry { /* ... */ }
```

### 步骤3：拆分工具实现
- 将每个工具类别的实现移到对应文件
- 保持函数签名不变
- 使用 `pub use` 重新导出

### 步骤4：优化JSON处理
在 `utils/json.rs` 中：
```rust
pub fn parse_tool_input<T: DeserializeOwned>(input: &Value) -> Result<T, String> {
    // 直接使用 Value，避免字符串转换
    serde_json::from_value(input.clone()).map_err(|e| e.to_string())
}

pub fn serialize_tool_output<T: Serialize>(output: T) -> Result<String, String> {
    // 只在最后一步序列化
    serde_json::to_string_pretty(&output).map_err(|e| e.to_string())
}
```

### 步骤5：优化权限检查
在 `utils/permission.rs` 中：
```rust
pub fn check_permission(
    enforcer: Option<&PermissionEnforcer>,
    tool_name: &str,
    input: &Value,  // 直接使用 Value，不转换为字符串
) -> Result<(), String> {
    // 只在需要时序列化
    if let Some(enforcer) = enforcer {
        let input_str = serde_json::to_string(input).unwrap_or_default();
        let result = enforcer.check(tool_name, &input_str);
        match result {
            EnforcementResult::Allowed => Ok(()),
            EnforcementResult::Denied { reason, .. } => Err(reason),
        }
    } else {
        Ok(())
    }
}
```

## 预期收益
1. **编译时间减少**：模块化编译，增量构建更快
2. **内存使用减少**：避免重复的JSON字符串
3. **执行速度提升**：减少序列化/反序列化开销
4. **代码可维护性**：逻辑分离，便于测试和修改

## 风险控制
1. 保持API兼容性，不改变外部接口
2. 分阶段实施，每阶段都有测试验证
3. 保留原有测试用例，确保功能正确性