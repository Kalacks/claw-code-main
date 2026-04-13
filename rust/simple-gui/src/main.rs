#![windows_subsystem = "windows"]

use eframe::egui;

struct SimpleChatApp {
    messages: Vec<String>,
    input_text: String,
}

impl Default for SimpleChatApp {
    fn default() -> Self {
        Self {
            messages: vec![
                "=== Claw Code 最小化 GUI ===".to_string(),
                "版本: 1.0.0".to_string(),
                "大小: ~120 KB".to_string(),
                "".to_string(),
                "这是一个独立的最小化 GUI 应用".to_string(),
                "基于 eframe/egui 构建".to_string(),
                "".to_string(),
                "输入消息并点击发送按钮".to_string(),
            ],
            input_text: String::new(),
        }
    }
}

impl eframe::App for SimpleChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Claw Code 最小化 GUI");
            ui.separator();
            
            // 显示消息区域
            ui.label("对话记录:");
            egui::ScrollArea::vertical()
                .max_height(350.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for message in &self.messages {
                        if message.is_empty() {
                            ui.separator();
                        } else {
                            ui.label(message);
                        }
                    }
                });
            
            ui.separator();
            
            // 输入区域
            ui.horizontal(|ui| {
                ui.label("消息:");
                ui.add(egui::TextEdit::multiline(&mut self.input_text)
                    .desired_rows(2)
                    .desired_width(300.0));
            });
            
            // 按钮区域
            ui.horizontal(|ui| {
                if ui.button("📤 发送").clicked() {
                    self.send_message();
                }
                
                if ui.button("🗑️ 清空").clicked() {
                    self.messages.clear();
                    self.messages.push("对话已清空".to_string());
                }
                
                if ui.button("ℹ️ 信息").clicked() {
                    self.messages.push("=== 应用信息 ===".to_string());
                    self.messages.push("名称: Claw Code 最小化 GUI".to_string());
                    self.messages.push("构建日期: 2026-03-31".to_string());
                    self.messages.push("大小: ~120 KB".to_string());
                    self.messages.push("框架: eframe/egui".to_string());
                }
                
                if ui.button("❌ 退出").clicked() {
                    std::process::exit(0);
                }
            });
            
            ui.separator();
            ui.label("快捷键: Enter = 发送, Ctrl+Q = 退出");
        });
        
        // 键盘快捷键
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.send_message();
        }
        
        if ctx.input(|i| i.key_pressed(egui::Key::Q) && i.modifiers.ctrl) {
            std::process::exit(0);
        }
    }
}

impl SimpleChatApp {
    fn send_message(&mut self) {
        let text = self.input_text.trim();
        if text.is_empty() {
            return;
        }
        
        self.messages.push(format!("你: {}", text));
        
        // 简单回复逻辑
        let response = if text.contains("你好") || text.contains("hi") || text.contains("hello") {
            "助手: 你好！有什么可以帮助你的吗？"
        } else if text.contains("时间") {
            "助手: 当前时间是 2026-03-31"
        } else if text.contains("帮助") {
            "助手: 这是一个最小化 GUI 演示应用。你可以输入任何消息。"
        } else {
            "助手: 已收到你的消息。这是一个演示回复。"
        };
        
        self.messages.push(response.to_string());
        self.input_text.clear();
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Claw Code 最小化 GUI")
            .with_inner_size([500.0, 600.0])
            .with_min_inner_size([400.0, 400.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "Claw Code 最小化 GUI",
        options,
        Box::new(|_cc| Ok(Box::<SimpleChatApp>::default())),
    )
}