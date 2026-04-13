#![windows_subsystem = "windows"]

use eframe::egui;

struct MinimalApp {
    input_text: String,
    output_text: String,
    messages: Vec<String>,
}

impl Default for MinimalApp {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            output_text: String::new(),
            messages: vec![
                "欢迎使用 Claw Code 最小化 GUI".to_string(),
                "这是一个简单的聊天界面".to_string(),
                "输入消息后点击发送".to_string(),
            ],
        }
    }
}

impl eframe::App for MinimalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Claw Code 最小化 GUI");
            ui.separator();

            // 显示消息历史
            ui.label("消息历史:");
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for message in &self.messages {
                        ui.label(message);
                    }
                });

            ui.separator();

            // 输入区域
            ui.horizontal(|ui| {
                ui.label("输入:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.input_text)
                        .desired_rows(3)
                        .desired_width(400.0),
                );
            });

            ui.horizontal(|ui| {
                if ui.button("发送").clicked() && !self.input_text.trim().is_empty() {
                    let user_msg = format!("用户: {}", self.input_text.trim());
                    self.messages.push(user_msg);

                    // 模拟回复
                    let bot_msg = format!("助手: 已收到你的消息: '{}'", self.input_text.trim());
                    self.messages.push(bot_msg);

                    self.input_text.clear();
                }

                if ui.button("清空").clicked() {
                    self.messages.clear();
                    self.messages.push("对话已清空".to_string());
                }

                if ui.button("退出").clicked() {
                    std::process::exit(0);
                }
            });

            ui.separator();
            ui.label("这是一个最小化的 GUI 示例，基于 eframe/egui 构建");
            ui.label("大小: ~5 MB (静态链接)");
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Claw Code 最小化 GUI")
            .with_inner_size([600.0, 500.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Claw Code 最小化 GUI",
        options,
        Box::new(|_cc| Ok(Box::<MinimalApp>::default())),
    )
}
