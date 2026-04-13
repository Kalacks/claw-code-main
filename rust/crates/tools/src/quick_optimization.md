# 关键函数优化方案

## 1. 优化 `execute_tool_with_enforcer` 函数

### 当前问题：
- 大型match语句（约80个分支）
- 每个分支都进行权限检查
- 重复的JSON序列化

### 优化方案：
```rust
// 使用HashMap预注册工具处理器
type ToolHandler = fn(&Value) -> Result<String, String>;

struct ToolRegistry {
    handlers: HashMap<&'static str, ToolHandler>,
    permission_required: HashMap<&'static str, PermissionMode>,
}

impl ToolRegistry {
    fn new() -> Self {
        let mut handlers = HashMap::new();
        let mut permission_required = HashMap::new();
        
        // 注册工具处理器
        handlers.insert("bash", run_bash as ToolHandler);
        handlers.insert("read_file", run_read_file as ToolHandler);
        // ... 其他工具
        
        // 注册权限要求
        permission_required.insert("bash", PermissionMode::DangerFullAccess);
        permission_required.insert("read_file", PermissionMode::ReadOnly);
        // ... 其他工具
        
        Self { handlers, permission_required }
    }
    
    fn execute(
        &self,
        enforcer: Option<&PermissionEnforcer>,
        name: &str,
        input: &Value,
    ) -> Result<String, String> {
        // 1. 查找处理器
        let handler = self.handlers.get(name)
            .ok_or_else(|| format!("unsupported tool: {name}"))?;
        
        // 2. 检查权限（优化版）
        if let Some(enforcer) = enforcer {
            if let Some(required_mode) = self.permission_required.get(name) {
                // 快速权限检查
                if !enforcer.is_allowed(name, required_mode) {
                    return Err(format!(
                        "tool `{name}` requires {required_mode:?} permission"
                    ));
                }
            }
        }
        
        // 3. 执行工具
        handler(input)
    }
}
```

## 2. 优化JSON处理

### 当前问题：
- 工具输入从字符串反序列化为Value
- 权限检查时又序列化为字符串
- 输出时再次序列化为字符串

### 优化方案：
```rust
// 使用缓存的结构化输入
#[derive(Clone)]
struct ToolInput {
    raw: String,           // 原始JSON字符串
    parsed: Option<Value>, // 懒解析的Value
}

impl ToolInput {
    fn new(raw: String) -> Self {
        Self { raw, parsed: None }
    }
    
    fn as_value(&mut self) -> Result<&Value, String> {
        if self.parsed.is_none() {
            self.parsed = Some(
                serde_json::from_str(&self.raw)
                    .map_err(|e| format!("invalid JSON: {e}"))?
            );
        }
        Ok(self.parsed.as_ref().unwrap())
    }
    
    fn as_str(&self) -> &str {
        &self.raw
    }
}

// 工具执行器使用优化后的输入
fn execute_tool_optimized(
    registry: &ToolRegistry,
    enforcer: Option<&PermissionEnforcer>,
    name: &str,
    input: ToolInput,
) -> Result<String, String> {
    // 权限检查使用原始字符串，避免解析
    if let Some(enforcer) = enforcer {
        let result = enforcer.check(name, input.as_str());
        if let EnforcementResult::Denied { reason, .. } = result {
            return Err(reason);
        }
    }
    
    // 执行时再解析
    let value = input.as_value()?;
    registry.execute_without_permission(name, value)
}
```

## 3. 优化字符串操作

### 当前问题：
- 频繁的字符串拼接（format!宏）
- 工具搜索中的规范化操作
- HTML解析中的字符串处理

### 优化方案：
```rust
// 使用字符串缓存
lazy_static! {
    static ref TOOL_NAME_CACHE: HashMap<&'static str, String> = {
        let mut map = HashMap::new();
        for spec in mvp_tool_specs() {
            let normalized = normalize_tool_name(spec.name);
            map.insert(spec.name, normalized);
        }
        map
    };
}

// 预编译正则表达式
lazy_static! {
    static ref HTML_TAG_REGEX: Regex = Regex::new(r"<[^>]*>").unwrap();
    static ref WHITESPACE_REGEX: Regex = Regex::new(r"\s+").unwrap();
}

// 使用Cow<'_, str>避免分配
fn normalize_tool_name_cached(name: &str) -> Cow<'_, str> {
    if let Some(cached) = TOOL_NAME_CACHE.get(name) {
        Cow::Borrowed(cached)
    } else {
        // 计算并缓存
        let normalized = name.trim().replace('-', "_").to_ascii_lowercase();
        Cow::Owned(normalized)
    }
}

// 批量处理字符串
fn process_multiple_strings(strings: &[String]) -> String {
    let mut result = String::with_capacity(strings.iter().map(|s| s.len()).sum());
    for s in strings {
        result.push_str(s);
    }
    result
}
```

## 4. 优化Agent相关函数

### 当前问题：
- `execute_agent` 函数过于复杂
- 多次文件系统操作
- 重复的状态序列化

### 优化方案：
```rust
// 拆分Agent执行流程
struct AgentExecutor {
    store_dir: PathBuf,
    model_resolver: ModelResolver,
    system_prompt_loader: SystemPromptLoader,
}

impl AgentExecutor {
    async fn execute(&self, input: AgentInput) -> Result<AgentOutput, String> {
        // 1. 验证输入（快速失败）
        self.validate_input(&input)?;
        
        // 2. 准备输出目录（一次性）
        let agent_id = self.generate_agent_id();
        let output_dir = self.prepare_output_dir(&agent_id)?;
        
        // 3. 并行加载资源
        let (model, system_prompt, allowed_tools) = tokio::try_join!(
            self.resolve_model(&input),
            self.load_system_prompt(&input),
            self.resolve_allowed_tools(&input),
        )?;
        
        // 4. 创建manifest（延迟写入）
        let manifest = self.create_manifest(
            &agent_id,
            &input,
            &model,
            &output_dir,
        );
        
        // 5. 写入文件（批量）
        self.write_agent_files(&output_dir, &manifest, &input.prompt)?;
        
        // 6. 启动子任务（异步）
        self.spawn_agent_task(manifest, input.prompt, system_prompt, allowed_tools)
            .await
    }
    
    fn validate_input(&self, input: &AgentInput) -> Result<(), String> {
        if input.description.trim().is_empty() {
            return Err("description must not be empty".to_string());
        }
        if input.prompt.trim().is_empty() {
            return Err("prompt must not be empty".to_string());
        }
        Ok(())
    }
}
```

## 5. 性能监控和优化

```rust
// 添加性能监控
#[derive(Default)]
struct PerformanceMetrics {
    tool_calls: usize,
    json_serializations: usize,
    string_allocations: usize,
    total_duration: Duration,
}

impl PerformanceMetrics {
    fn record_tool_call<F>(&mut self, name: &str, f: F) -> Result<String, String>
    where
        F: FnOnce() -> Result<String, String>,
    {
        let start = Instant::now();
        self.tool_calls += 1;
        
        let result = f();
        
        self.total_duration += start.elapsed();
        
        // 记录到日志（可选）
        if self.tool_calls % 100 == 0 {
            log::debug!(
                "Tool performance: {} calls, avg {:.2}ms",
                self.tool_calls,
                self.total_duration.as_millis() as f64 / self.tool_calls as f64
            );
        }
        
        result
    }
}
```

## 实施优先级

1. **高优先级**（立即实施）：
   - 优化 `execute_tool_with_enforcer` 使用HashMap
   - 添加JSON处理缓存
   - 预编译正则表达式

2. **中优先级**（一周内）：
   - 拆分Agent相关函数
   - 优化字符串操作
   - 添加性能监控

3. **低优先级**（长期）：
   - 完整模块化重构
   - 异步工具执行
   - 更细粒度的缓存策略

## 预期效果

| 优化项 | 预计性能提升 | 代码复杂度变化 |
|--------|--------------|----------------|
| HashMap工具查找 | 30-50% | 略微增加 |
| JSON缓存 | 20-40% | 略微增加 |
| 字符串优化 | 10-30% | 基本不变 |
| Agent拆分 | 15-25% | 降低 |
| 总体验证 | 25-40% | 总体降低 |

## 测试策略

1. 保持所有现有测试通过
2. 添加性能基准测试
3. 内存使用监控
4. 回归测试确保功能正确性