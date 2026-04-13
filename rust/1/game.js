// 游戏常量
const GRID_SIZE = 20;
const CANVAS_WIDTH = 600;
const CANVAS_HEIGHT = 600;
const GRID_WIDTH = CANVAS_WIDTH / GRID_SIZE;
const GRID_HEIGHT = CANVAS_HEIGHT / GRID_SIZE;

// 游戏状态
let game = {
    snake: [],
    food: { x: 0, y: 0 },
    direction: 'right',
    nextDirection: 'right',
    score: 0,
    highScore: localStorage.getItem('snakeHighScore') || 0,
    gameOver: false,
    paused: false,
    gameLoop: null,
    speed: 150, // 初始速度（毫秒）
    difficulty: 'easy'
};

// DOM元素
const canvas = document.getElementById('game-canvas');
const ctx = canvas.getContext('2d');
const scoreElement = document.getElementById('score');
const highScoreElement = document.getElementById('high-score');
const lengthElement = document.getElementById('length');
const startBtn = document.getElementById('start-btn');
const pauseBtn = document.getElementById('pause-btn');
const resetBtn = document.getElementById('reset-btn');
const restartBtn = document.getElementById('restart-btn');
const gameOverElement = document.getElementById('game-over');
const gamePausedElement = document.getElementById('game-paused');
const finalScoreElement = document.getElementById('final-score');
const difficultyRadios = document.querySelectorAll('input[name="difficulty"]');

// 初始化游戏
function initGame() {
    // 初始化蛇
    game.snake = [
        { x: 10, y: 10 },
        { x: 9, y: 10 },
        { x: 8, y: 10 }
    ];
    
    // 生成食物
    generateFood();
    
    // 重置游戏状态
    game.direction = 'right';
    game.nextDirection = 'right';
    game.score = 0;
    game.gameOver = false;
    game.paused = false;
    
    // 更新UI
    updateScore();
    gameOverElement.classList.remove('active');
    gamePausedElement.classList.remove('active');
    
    // 绘制初始状态
    draw();
}

// 生成食物
function generateFood() {
    let foodPosition;
    let validPosition = false;
    
    while (!validPosition) {
        foodPosition = {
            x: Math.floor(Math.random() * GRID_WIDTH),
            y: Math.floor(Math.random() * GRID_HEIGHT)
        };
        
        // 检查食物是否与蛇身重叠
        validPosition = !game.snake.some(segment => 
            segment.x === foodPosition.x && segment.y === foodPosition.y
        );
    }
    
    game.food = foodPosition;
}

// 绘制游戏
function draw() {
    // 清空画布
    ctx.fillStyle = '#0f1123';
    ctx.fillRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
    
    // 绘制网格
    drawGrid();
    
    // 绘制蛇
    drawSnake();
    
    // 绘制食物
    drawFood();
}

// 绘制网格
function drawGrid() {
    ctx.strokeStyle = 'rgba(76, 201, 240, 0.1)';
    ctx.lineWidth = 1;
    
    // 绘制垂直线
    for (let x = 0; x <= CANVAS_WIDTH; x += GRID_SIZE) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, CANVAS_HEIGHT);
        ctx.stroke();
    }
    
    // 绘制水平线
    for (let y = 0; y <= CANVAS_HEIGHT; y += GRID_SIZE) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(CANVAS_WIDTH, y);
        ctx.stroke();
    }
}

// 绘制蛇
function drawSnake() {
    game.snake.forEach((segment, index) => {
        // 蛇头
        if (index === 0) {
            ctx.fillStyle = '#4cc9f0';
            ctx.fillRect(
                segment.x * GRID_SIZE, 
                segment.y * GRID_SIZE, 
                GRID_SIZE, 
                GRID_SIZE
            );
            
            // 蛇头边框
            ctx.strokeStyle = '#fff';
            ctx.lineWidth = 2;
            ctx.strokeRect(
                segment.x * GRID_SIZE, 
                segment.y * GRID_SIZE, 
                GRID_SIZE, 
                GRID_SIZE
            );
            
            // 绘制眼睛
            ctx.fillStyle = '#fff';
            let eyeSize = GRID_SIZE / 5;
            
            // 根据方向调整眼睛位置
            let leftEyeX, leftEyeY, rightEyeX, rightEyeY;
            
            switch(game.direction) {
                case 'right':
                    leftEyeX = segment.x * GRID_SIZE + GRID_SIZE - eyeSize * 2;
                    leftEyeY = segment.y * GRID_SIZE + eyeSize * 2;
                    rightEyeX = segment.x * GRID_SIZE + GRID_SIZE - eyeSize * 2;
                    rightEyeY = segment.y * GRID_SIZE + GRID_SIZE - eyeSize * 3;
                    break;
                case 'left':
                    leftEyeX = segment.x * GRID_SIZE + eyeSize;
                    leftEyeY = segment.y * GRID_SIZE + eyeSize * 2;
                    rightEyeX = segment.x * GRID_SIZE + eyeSize;
                    rightEyeY = segment.y * GRID_SIZE + GRID_SIZE - eyeSize * 3;
                    break;
                case 'up':
                    leftEyeX = segment.x * GRID_SIZE + eyeSize * 2;
                    leftEyeY = segment.y * GRID_SIZE + eyeSize;
                    rightEyeX = segment.x * GRID_SIZE + GRID_SIZE - eyeSize * 3;
                    rightEyeY = segment.y * GRID_SIZE + eyeSize;
                    break;
                case 'down':
                    leftEyeX = segment.x * GRID_SIZE + eyeSize * 2;
                    leftEyeY = segment.y * GRID_SIZE + GRID_SIZE - eyeSize * 2;
                    rightEyeX = segment.x * GRID_SIZE + GRID_SIZE - eyeSize * 3;
                    rightEyeY = segment.y * GRID_SIZE + GRID_SIZE - eyeSize * 2;
                    break;
            }
            
            ctx.fillRect(leftEyeX, leftEyeY, eyeSize, eyeSize);
            ctx.fillRect(rightEyeX, rightEyeY, eyeSize, eyeSize);
            
            // 瞳孔
            ctx.fillStyle = '#000';
            ctx.fillRect(leftEyeX + eyeSize/4, leftEyeY + eyeSize/4, eyeSize/2, eyeSize/2);
            ctx.fillRect(rightEyeX + eyeSize/4, rightEyeY + eyeSize/4, eyeSize/2, eyeSize/2);
        } 
        // 蛇身
        else {
            // 渐变颜色
            const gradient = ctx.createLinearGradient(
                segment.x * GRID_SIZE, 
                segment.y * GRID_SIZE, 
                segment.x * GRID_SIZE + GRID_SIZE, 
                segment.y * GRID_SIZE + GRID_SIZE
            );
            
            gradient.addColorStop(0, '#4361ee');
            gradient.addColorStop(1, '#3a0ca3');
            
            ctx.fillStyle = gradient;
            ctx.fillRect(
                segment.x * GRID_SIZE, 
                segment.y * GRID_SIZE, 
                GRID_SIZE, 
                GRID_SIZE
            );
            
            // 蛇身边框
            ctx.strokeStyle = '#4cc9f0';
            ctx.lineWidth = 1;
            ctx.strokeRect(
                segment.x * GRID_SIZE, 
                segment.y * GRID_SIZE, 
                GRID_SIZE, 
                GRID_SIZE
            );
        }
    });
}

// 绘制食物
function drawFood() {
    // 创建径向渐变
    const gradient = ctx.createRadialGradient(
        game.food.x * GRID_SIZE + GRID_SIZE / 2,
        game.food.y * GRID_SIZE + GRID_SIZE / 2,
        0,
        game.food.x * GRID_SIZE + GRID_SIZE / 2,
        game.food.y * GRID_SIZE + GRID_SIZE / 2,
        GRID_SIZE / 2
    );
    
    gradient.addColorStop(0, '#ff0000');
    gradient.addColorStop(0.7, '#cc0000');
    gradient.addColorStop(1, '#990000');
    
    ctx.fillStyle = gradient;
    ctx.beginPath();
    ctx.arc(
        game.food.x * GRID_SIZE + GRID_SIZE / 2,
        game.food.y * GRID_SIZE + GRID_SIZE / 2,
        GRID_SIZE / 2 - 2,
        0,
        Math.PI * 2
    );
    ctx.fill();
    
    // 食物高光
    ctx.fillStyle = 'rgba(255, 255, 255, 0.3)';
    ctx.beginPath();
    ctx.arc(
        game.food.x * GRID_SIZE + GRID_SIZE / 3,
        game.food.y * GRID_SIZE + GRID_SIZE / 3,
        GRID_SIZE / 6,
        0,
        Math.PI * 2
    );
    ctx.fill();
}

// 更新游戏状态
function update() {
    if (game.gameOver || game.paused) return;
    
    // 更新方向
    game.direction = game.nextDirection;
    
    // 计算新的蛇头位置
    const head = { ...game.snake[0] };
    
    switch (game.direction) {
        case 'up':
            head.y -= 1;
            break;
        case 'down':
            head.y += 1;
            break;
        case 'left':
            head.x -= 1;
            break;
        case 'right':
            head.x += 1;
            break;
    }
    
    // 检查碰撞
    if (checkCollision(head)) {
        gameOver();
        return;
    }
    
    // 添加新的蛇头
    game.snake.unshift(head);
    
    // 检查是否吃到食物
    if (head.x === game.food.x && head.y === game.food.y) {
        // 增加分数
        game.score += 1;
        
        // 更新最高分
        if (game.score > game.highScore) {
            game.highScore = game.score;
            localStorage.setItem('snakeHighScore', game.highScore);
        }
        
        // 更新UI
        updateScore();
        
        // 生成新食物
        generateFood();
        
        // 根据难度调整速度
        adjustSpeed();
    } else {
        // 如果没有吃到食物，移除蛇尾
        game.snake.pop();
    }
    
    // 重新绘制
    draw();
}

// 检查碰撞
function checkCollision(head) {
    // 检查墙壁碰撞
    if (
        head.x < 0 || 
        head.x >= GRID_WIDTH || 
        head.y < 0 || 
        head.y >= GRID_HEIGHT
    ) {
        return true;
    }
    
    // 检查自身碰撞
    for (let i = 0; i < game.snake.length; i++) {
        if (game.snake[i].x === head.x && game.snake[i].y === head.y) {
            return true;
        }
    }
    
    return false;
}

// 游戏结束
function gameOver() {
    game.gameOver = true;
    clearInterval(game.gameLoop);
    
    // 显示游戏结束界面
    finalScoreElement.textContent = `得分: ${game.score}`;
    gameOverElement.classList.add('active');
    
    // 添加动画效果
    gameOverElement.classList.add('pulse');
    setTimeout(() => {
        gameOverElement.classList.remove('pulse');
    }, 500);
}

// 更新分数显示
function updateScore() {
    scoreElement.textContent = game.score;
    highScoreElement.textContent = game.highScore;
    lengthElement.textContent = game.snake.length;
    
    // 添加动画效果
    scoreElement.classList.add('pulse');
    setTimeout(() => {
        scoreElement.classList.remove('pulse');
    }, 300);
}

// 根据难度调整速度
function adjustSpeed() {
    switch (game.difficulty) {
        case 'easy':
            game.speed = Math.max(100, 150 - Math.floor(game.score / 5) * 10);
            break;
        case 'medium':
            game.speed = Math.max(80, 120 - Math.floor(game.score / 5) * 10);
            break;
        case 'hard':
            game.speed = Math.max(60, 100 - Math.floor(game.score / 5) * 10);
            break;
    }
    
    // 如果游戏正在运行，重新设置游戏循环
    if (game.gameLoop) {
        clearInterval(game.gameLoop);
        game.gameLoop = setInterval(update, game.speed);
    }
}

// 开始游戏
function startGame() {
    if (game.gameLoop) {
        clearInterval(game.gameLoop);
    }
    
    game.paused = false;
    gamePausedElement.classList.remove('active');
    
    game.gameLoop = setInterval(update, game.speed);
    startBtn.disabled = true;
    pauseBtn.disabled = false;
}

// 暂停游戏
function pauseGame() {
    if (game.gameOver) return;
    
    game.paused = !game.paused;
    
    if (game.paused) {
        clearInterval(game.gameLoop);
        gamePausedElement.classList.add('active');
        pauseBtn.innerHTML = '<i class="fas fa-play"></i> 继续';
    } else {
        game.gameLoop = setInterval(update, game.speed);
        gamePausedElement.classList.remove('active');
        pauseBtn.innerHTML = '<i class="fas fa-pause"></i> 暂停';
    }
}

// 重置游戏
function resetGame() {
    clearInterval(game.gameLoop);
    game.gameLoop = null;
    
    // 获取当前难度设置
    const selectedDifficulty = document.querySelector('input[name="difficulty"]:checked').value;
    game.difficulty = selectedDifficulty;
    
    // 根据难度设置初始速度
    switch (selectedDifficulty) {
        case 'easy':
            game.speed = 150;
            break;
        case 'medium':
            game.speed = 120;
            break;
        case 'hard':
            game.speed = 100;
            break;
    }
    
    initGame();
    startBtn.disabled = false;
    pauseBtn.disabled = true;
    pauseBtn.innerHTML = '<i class="fas fa-pause"></i> 暂停';
    gamePausedElement.classList.remove('active');
}

// 键盘控制
function handleKeyDown(event) {
    switch (event.key) {
        case 'ArrowUp':
            if (game.direction !== 'down') {
                game.nextDirection = 'up';
            }
            break;
        case 'ArrowDown':
            if (game.direction !== 'up') {
                game.nextDirection = 'down';
            }
            break;
        case 'ArrowLeft':
            if (game.direction !== 'right') {
                game.nextDirection = 'left';
            }
            break;
        case 'ArrowRight':
            if (game.direction !== 'left') {
                game.nextDirection = 'right';
            }
            break;
        case ' ':
        case 'Spacebar':
            event.preventDefault();
            if (game.gameLoop) {
                pauseGame();
            }
            break;
        case 'Enter':
            if (game.gameOver) {
                resetGame();
                startGame();
            }
            break;
    }
}

// 事件监听器
startBtn.addEventListener('click', () => {
    if (!game.gameLoop) {
        startGame();
    }
});

pauseBtn.addEventListener('click', pauseGame);

resetBtn.addEventListener('click', resetGame);

restartBtn.addEventListener('click', () => {
    resetGame();
    startGame();
});

// 难度设置变化
difficultyRadios.forEach(radio => {
    radio.addEventListener('change', (event) => {
        if (game.gameLoop) {
            // 如果游戏正在运行，询问用户是否要重新开始
            if (confirm('更改难度将重新开始游戏，确定要继续吗？')) {
                resetGame();
            } else {
                // 恢复之前的选项
                document.querySelector(`input[name="difficulty"][value="${game.difficulty}"]`).checked = true;
            }
        } else {
            game.difficulty = event.target.value;
        }
    });
});

// 触摸控制（移动设备）
let touchStartX = 0;
let touchStartY = 0;

canvas.addEventListener('touchstart', (event) => {
    event.preventDefault();
    touchStartX = event.touches[0].clientX;
    touchStartY = event.touches[0].clientY;
});

canvas.addEventListener('touchmove', (event) => {
    event.preventDefault();
});

canvas.addEventListener('touchend', (event) => {
    event.preventDefault();
    const touchEndX = event.changedTouches[0].clientX;
    const touchEndY = event.changedTouches[0].clientY;
    
    const dx = touchEndX - touchStartX;
    const dy = touchEndY - touchStartY;
    
    // 确定滑动方向
    if (Math.abs(dx) > Math.abs(dy)) {
        // 水平滑动
        if (dx > 0 && game.direction !== 'left') {
            game.nextDirection = 'right';
        } else if (dx < 0 && game.direction !== 'right') {
            game.nextDirection = 'left';
        }
    } else {
        // 垂直滑动
        if (dy > 0 && game.direction !== 'up') {
            game.nextDirection = 'down';
        } else if (dy < 0 && game.direction !== 'down') {
            game.nextDirection = 'up';
        }
    }
});

// 初始化
document.addEventListener('keydown', handleKeyDown);
highScoreElement.textContent = game.highScore;
initGame();

// 添加游戏说明动画
const instructions = document.querySelectorAll('.instructions li');
instructions.forEach((item, index) => {
    item.style.animationDelay = `${index * 0.1}s`;
    item.classList.add('animate-in');
});

console.log('贪吃蛇游戏已加载！使用方向键控制蛇的移动，空格键暂停/继续。');