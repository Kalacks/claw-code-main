use std::path::PathBuf;

use eframe::egui::{self, Align, ComboBox, Layout, RichText, TextEdit};

use super::{ClawGuiApp, Language, Tab, UiTheme};

pub fn configure_gui_fonts(ctx: &egui::Context) {
    let Some((font_name, font_bytes)) = load_cjk_font() else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        font_name.clone(),
        egui::FontData::from_owned(font_bytes).into(),
    );

    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, font_name.clone());
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.push(font_name);
    }

    ctx.set_fonts(fonts);
}

fn load_cjk_font() -> Option<(String, Vec<u8>)> {
    for path in preferred_cjk_font_paths() {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        let name = path
            .file_stem()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "claw-cjk".to_string());
        return Some((format!("claw-{name}"), bytes));
    }
    None
}

fn preferred_cjk_font_paths() -> Vec<PathBuf> {
    let windows_fonts = std::env::var("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\Windows"))
        .join("Fonts");

    vec![
        windows_fonts.join("NotoSansSC-VF.ttf"),
        windows_fonts.join("Noto Sans SC (TrueType).otf"),
        windows_fonts.join("simhei.ttf"),
        windows_fonts.join("Deng.ttf"),
        windows_fonts.join("msyh.ttc"),
    ]
}

pub fn render_help_window_v3(app: &mut ClawGuiApp, ctx: &egui::Context) {
    if !app.show_help {
        return;
    }

    let title = app.tr("帮助与说明", "Help & Notes").to_string();
    let mut open = app.show_help;
    egui::Window::new(title)
        .open(&mut open)
        .default_width(620.0)
        .show(ctx, |ui| {
            ui.label(app.tr(
                "这是一个面向多模型编码 Agent 的桌面 GUI。左侧管理文件夹和线程，中间进行多轮对话，右侧查看工具事件、Token 和费用。",
                "This desktop GUI is built for multi-model coding agents. Use the left side for folders and threads, the center for chat, and the right side for tool events, tokens, and cost.",
            ));
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label(
                RichText::new(app.tr("推荐流程", "Recommended flow"))
                    .strong()
                    .color(app.theme.accent()),
            );
            ui.label(app.tr(
                "1. 先在“模型”页保存并激活一个 LLM Profile。",
                "1. Start in Models and save an LLM profile, then activate it.",
            ));
            ui.label(app.tr(
                "2. 在左侧导入文件夹并创建线程，让不同项目保持独立上下文。",
                "2. Import folders and create threads on the left so different projects keep separate context.",
            ));
            ui.label(app.tr(
                "3. 在中间对话区持续多轮聊天，按 Ctrl+Enter 可以直接发送。",
                "3. Keep chatting in the center panel; press Ctrl+Enter to send.",
            ));
            ui.label(app.tr(
                "4. 右侧 Inspector 会显示连接信息、Token、人民币或美元费用，以及工具事件。",
                "4. The right inspector shows connection info, token usage, RMB or USD cost, and tool events.",
            ));
            ui.label(app.tr(
                "5. “应用”页适合保存常用命令入口，“会话”页适合回看和恢复历史会话。",
                "5. Use Apps for reusable commands and Sessions for loading or resuming saved workspace sessions.",
            ));
        });
    app.show_help = open;
}

pub fn render_top_bar_v4(app: &mut ClawGuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("claw_gui_top_bar_v4").show(ctx, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Claw Client")
                    .strong()
                    .size(22.0)
                    .color(app.theme.accent()),
            );
            ui.label(
                RichText::new(app.tr(
                    "接近 Codex 桌面端的多模型工作台",
                    "A multi-model workspace inspired by Codex Desktop",
                ))
                .small(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(app.tr("帮助", "Help")).clicked() {
                    app.show_help = true;
                }

                ComboBox::from_id_salt("gui_theme_v4")
                    .selected_text(theme_label(app.theme, app.language))
                    .show_ui(ui, |ui| {
                        for theme in [
                            UiTheme::Sand,
                            UiTheme::Mist,
                            UiTheme::Forest,
                            UiTheme::Graphite,
                        ] {
                            ui.selectable_value(
                                &mut app.theme,
                                theme,
                                theme_label(theme, app.language),
                            );
                        }
                    });

                ComboBox::from_id_salt("gui_lang_v4")
                    .selected_text(language_label(app.language))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut app.language,
                            Language::Zh,
                            language_label(Language::Zh),
                        );
                        ui.selectable_value(
                            &mut app.language,
                            Language::En,
                            language_label(Language::En),
                        );
                    });
            });
        });

        ui.horizontal_wrapped(|ui| {
            for tab in Tab::all() {
                let selected = app.active_tab == tab;
                if ui
                    .selectable_label(selected, tab_label(tab, app.language))
                    .clicked()
                {
                    app.active_tab = tab;
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label(app.tr("工作区", "Workspace"));
            let input_width = (ui.available_width() - 300.0).max(180.0);
            let response = ui.add_sized(
                [input_width, 28.0],
                TextEdit::singleline(&mut app.workspace_input),
            );
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                app.reload();
            }
            if ui.button(app.tr("加载", "Load")).clicked() {
                app.reload();
            }
            if ui.button(app.tr("工作区对话", "Workspace Chat")).clicked() {
                app.select_workspace_chat();
            }
            if ui.button(app.tr("会话", "Sessions")).clicked() {
                app.active_tab = Tab::Sessions;
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "{}: {}",
                app.tr("模型", "Model"),
                app.active_model()
            ));
            ui.separator();
            ui.label(format!(
                "{}: {}",
                app.tr("Profile", "Profile"),
                app.active_profile_name()
                    .unwrap_or_else(|| app.tr("(未激活)", "(none)").to_string())
            ));
            ui.separator();
            ui.label(format!(
                "{}: {}",
                app.tr("Provider", "Provider"),
                app.active_provider_clean()
            ));
            ui.separator();
            ui.label(format!(
                "{}: {}",
                app.tr("Base URL", "Base URL"),
                app.active_base_url_clean()
            ));
            ui.separator();
            ui.label(format!(
                "{}: {}",
                app.tr("当前线程", "Current thread"),
                app.active_thread_name
                    .clone()
                    .unwrap_or_else(|| app.tr("工作区对话", "Workspace Chat").to_string())
            ));
        });

        if let Some(error) = &app.error {
            ui.colored_label(egui::Color32::from_rgb(180, 48, 48), error);
        } else if let Some(notice) = &app.notice {
            ui.colored_label(app.theme.accent(), notice);
        }

        ui.add_space(4.0);
    });
}

fn tab_label(tab: Tab, language: Language) -> &'static str {
    match tab {
        Tab::Chat => language.pick("聊天", "Chat"),
        Tab::Models => language.pick("模型", "Models"),
        Tab::Apps => language.pick("应用", "Apps"),
        Tab::Sessions => language.pick("会话", "Sessions"),
    }
}

fn theme_label(theme: UiTheme, language: Language) -> &'static str {
    match theme {
        UiTheme::Sand => language.pick("暖沙", "Sand"),
        UiTheme::Mist => language.pick("薄雾", "Mist"),
        UiTheme::Forest => language.pick("森林", "Forest"),
        UiTheme::Graphite => language.pick("石墨", "Graphite"),
    }
}

fn language_label(language: Language) -> &'static str {
    match language {
        Language::Zh => "中文",
        Language::En => "English",
    }
}
