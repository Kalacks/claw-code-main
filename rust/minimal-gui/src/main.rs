use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([300.0, 200.0])
            .with_title("Minimal GUI"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Minimal GUI",
        options,
        Box::new(|_cc| Box::<MyApp>::default()),
    )
}

#[derive(Default)]
struct MyApp {
    name: String,
    count: u32,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Minimal GUI Application");
            ui.separator();
            
            ui.horizontal(|ui| {
                ui.label("Your name: ");
                ui.text_edit_singleline(&mut self.name);
            });
            
            ui.horizontal(|ui| {
                ui.label("Count: ");
                ui.label(format!("{}", self.count));
                if ui.button("+").clicked() {
                    self.count += 1;
                }
                if ui.button("-").clicked() && self.count > 0 {
                    self.count -= 1;
                }
                if ui.button("Reset").clicked() {
                    self.count = 0;
                }
            });
            
            ui.separator();
            
            if !self.name.is_empty() {
                ui.label(format!("Hello, {}!", self.name));
            } else {
                ui.label("Please enter your name");
            }
            
            ui.separator();
            
            ui.label("This is a minimal GUI application created with eframe/egui.");
            
            if ui.button("Quit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}