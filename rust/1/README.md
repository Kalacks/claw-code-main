# 贪吃蛇游戏

一个使用HTML5 Canvas、CSS3和JavaScript制作的现代化贪吃蛇游戏。

## 功能特点

- 🎮 完整的贪吃蛇游戏玩法
- 🎨 现代化的UI设计，响应式布局
- ⚙️ 三种难度级别（简单、中等、困难）
- 📊 实时分数显示和最高分记录
- 🎯 触摸屏支持（移动设备）
- ⏸️ 游戏暂停/继续功能
- 💾 本地存储最高分
- 📱 移动设备适配

## 游戏控制

### 键盘控制
- **方向键**：控制蛇的移动方向
- **空格键**：暂停/继续游戏
- **回车键**：游戏结束后重新开始

### 触摸控制（移动设备）
- **滑动屏幕**：控制蛇的移动方向

## 游戏规则

1. 使用方向键控制蛇的移动
2. 吃到红色食物可以增加1分，蛇身增长
3. 避免撞到墙壁或自己的身体
4. 随着分数增加，游戏速度会逐渐加快
5. 游戏结束后可以重新开始

## 难度设置

- **简单**：初始速度较慢，适合新手
- **中等**：平衡的速度和挑战性
- **困难**：初始速度较快，适合高手

## 文件结构

```
贪吃蛇游戏/
├── snake.html      # 主HTML文件
├── style.css       # 样式文件
└── game.js         # 游戏逻辑文件
```

## 运行方法

1. 直接双击 `snake.html` 文件在浏览器中打开
2. 或者使用任何HTTP服务器（如Live Server、Python SimpleHTTPServer等）运行

### 使用Python快速运行
```bash
# Python 3
python -m http.server

# Python 2
python -m SimpleHTTPServer
```

然后在浏览器中访问 `http://localhost:8000/snake.html`

## 技术栈

- **HTML5 Canvas**：游戏图形渲染
- **CSS3**：现代化UI设计，响应式布局
- **JavaScript (ES6)**：游戏逻辑和控制
- **Font Awesome**：图标库
- **LocalStorage**：最高分存储

## 浏览器兼容性

- Chrome 60+
- Firefox 55+
- Safari 11+
- Edge 79+
- iOS Safari 11+
- Android Chrome 60+

## 开发者说明

游戏的主要逻辑在 `game.js` 中实现，包括：
- 蛇的移动和碰撞检测
- 食物生成和得分计算
- 游戏状态管理
- 键盘和触摸事件处理

样式设计在 `style.css` 中，使用了：
- CSS Grid 和 Flexbox 布局
- 渐变和阴影效果
- 动画和过渡效果
- 响应式设计

## 许可证

MIT License - 可以自由使用、修改和分发。