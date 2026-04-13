use eframe::egui::{self, Frame, RichText, ScrollArea, Stroke, TextEdit};

use super::{section_card, AppForm, ClawGuiApp, LlmForm, LlmProfile, TurnTokenLimits};
use crate::agent_layer::AgentWorkspaceStore;
use crate::llm_layer::LlmProfileStore;

pub(super) fn render_models_tab_v5(app: &mut ClawGuiApp, ctx: &egui::Context) {
    let profiles = app
        .llm_store
        .as_ref()
        .map(LlmProfileStore::list_profiles)
        .unwrap_or_default();
    let limits_summary = app
        .llm_store
        .as_ref()
        .map_or_else(TurnTokenLimits::default, LlmProfileStore::turn_token_limits)
        .summary_line();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.columns(2, |columns| {
            section_card(
                &mut columns[0],
                app.theme,
                app.tr("模型配置", "Model Profile"),
                |ui| {
                    ui.label(app.tr("名称", "Name"));
                    ui.text_edit_singleline(&mut app.llm_form.name);
                    ui.label(app.tr("Provider", "Provider"));
                    ui.text_edit_singleline(&mut app.llm_form.provider);
                    ui.label(app.tr("Model", "Model"));
                    ui.text_edit_singleline(&mut app.llm_form.model);
                    ui.label(app.tr("Base URL", "Base URL"));
                    ui.text_edit_singleline(&mut app.llm_form.base_url);
                    let mut key_from_env = app.llm_key_from_env;

                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut key_from_env,
                            false,
                            app.tr("直接填入 Key", "Inline key"),
                        );
                        ui.selectable_value(
                            &mut key_from_env,
                            true,
                            app.tr("环境变量", "Env var"),
                        );
                    });
                    app.llm_key_from_env = key_from_env;

                    if app.llm_form.api_key_env.trim().is_empty() {
                        app.llm_form.api_key_env = app.derive_env_var_name();
                    }
                    ui.label(app.tr("环境变量名", "Environment variable"));
                    ui.text_edit_singleline(&mut app.llm_form.api_key_env);

                    if app.llm_key_from_env {
                        ui.small(app.tr(
                            "当前 profile 会优先从上面的环境变量读取 Key。",
                            "This profile will read its API key from the environment variable above.",
                        ));
                    }

                    ui.label(app.tr("API Key", "API key"));
                    ui.horizontal(|ui| {
                        ui.add(
                            TextEdit::singleline(&mut app.llm_form.api_key)
                                .password(!app.show_api_key)
                                .desired_width(ui.available_width() - 90.0),
                        );
                        if ui
                            .button(app.tr(
                                if app.show_api_key { "隐藏" } else { "显示" },
                                if app.show_api_key { "Hide" } else { "Show" },
                            ))
                            .clicked()
                        {
                            app.show_api_key = !app.show_api_key;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button(app.tr("写入当前环境", "Write env")).clicked() {
                            app.write_api_key_to_env_action(false);
                        }
                        if ui
                            .button(app.tr("写入并持久化", "Write + persist"))
                            .clicked()
                        {
                            app.write_api_key_to_env_action(true);
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.button(app.tr("保存", "Save")).clicked() {
                            app.save_profile_action();
                        }
                        if ui.button(app.tr("保存并激活", "Save + Activate")).clicked() {
                            let profile_name = app.llm_form.name.trim().to_string();
                            if app.save_profile_action() {
                                app.switch_profile_action(Some(&profile_name));
                            }
                        }
                        if ui.button(app.tr("清空表单", "Clear form")).clicked() {
                            app.llm_form = LlmForm::default();
                            app.llm_key_from_env = false;
                        }
                    });
                },
            );

            section_card(
                &mut columns[1],
                app.theme,
                app.tr("已导入模型", "Imported Profiles"),
                |ui| {
                    ui.label(format!(
                        "{}: {}",
                        app.tr("当前限额", "Current limits"),
                        limits_summary
                    ));
                    ui.add_space(4.0);

                    if profiles.is_empty() {
                        ui.label(app.tr(
                            "还没有已保存模型。先输入 Key 并保存一个模型配置。",
                            "No saved profiles yet. Enter a key and save a model profile first.",
                        ));
                    } else {
                        let mut activate_name = None::<String>;
                        let mut remove_name = None::<String>;
                        let mut edit_profile = None::<LlmProfile>;

                        for profile in profiles {
                            let active =
                                app.active_profile_name().as_deref() == Some(profile.name.as_str());
                            Frame::group(ui.style())
                                .fill(app.theme.subpanel_fill())
                                .stroke(Stroke::new(1.0, app.theme.accent().gamma_multiply(0.30)))
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(&profile.name).strong().color(if active {
                                                app.theme.accent()
                                            } else {
                                                ui.visuals().text_color()
                                            }),
                                        );
                                        if active {
                                            ui.label(app.tr("当前激活", "Active"));
                                        }
                                    });
                                    ui.small(format!(
                                        "provider={} model={}",
                                        profile.normalized_provider(),
                                        profile.model
                                    ));
                                    ui.small(format!(
                                        "{}: {}",
                                        app.tr("Key 来源", "Key source"),
                                        profile.key_source_label()
                                    ));
                                    ui.small(format!(
                                        "{}: {}",
                                        app.tr("Key 预览", "Key preview"),
                                        profile.masked_key_preview()
                                    ));
                                    ui.horizontal(|ui| {
                                        if ui.button(app.tr("编辑", "Edit")).clicked() {
                                            edit_profile = Some(profile.clone());
                                        }
                                        if ui.button(app.tr("激活", "Activate")).clicked() {
                                            activate_name = Some(profile.name.clone());
                                        }
                                        if ui.button(app.tr("删除", "Remove")).clicked() {
                                            remove_name = Some(profile.name.clone());
                                        }
                                    });
                                });
                            ui.add_space(6.0);
                        }

                        if let Some(profile) = edit_profile {
                            app.load_profile_into_form(&profile);
                        }
                        if let Some(name) = activate_name {
                            app.switch_profile_action(Some(&name));
                        }
                        if let Some(name) = remove_name {
                            app.remove_profile(&name);
                        }
                    }
                },
            );
        });
    });
}

pub(super) fn render_apps_tab_v3(app: &mut ClawGuiApp, ctx: &egui::Context) {
    let apps = app
        .agent_store
        .as_ref()
        .map(AgentWorkspaceStore::list_apps)
        .unwrap_or_default();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.columns(2, |columns| {
            section_card(
                &mut columns[0],
                app.theme,
                app.tr("保存应用命令", "Save App Command"),
                |ui| {
                    ui.label(app.tr("名称", "Name"));
                    ui.text_edit_singleline(&mut app.app_form.name);
                    ui.label(app.tr("命令", "Command"));
                    ui.text_edit_singleline(&mut app.app_form.command);
                    ui.label(app.tr("说明", "Description"));
                    ui.add(TextEdit::multiline(&mut app.app_form.description).desired_rows(4));
                    ui.horizontal(|ui| {
                        if ui.button(app.tr("保存应用", "Save app")).clicked() {
                            app.save_app_action();
                        }
                        if ui.button(app.tr("清空表单", "Clear form")).clicked() {
                            app.app_form = AppForm::default();
                        }
                    });
                },
            );

            section_card(
                &mut columns[1],
                app.theme,
                app.tr("已保存的应用", "Saved Apps"),
                |ui| {
                    if apps.is_empty() {
                        ui.label(app.tr(
                            "还没有已保存的应用。这里适合放常用命令、启动脚本和操作说明。",
                            "No saved apps yet. This area is good for reusable commands and notes.",
                        ));
                    } else {
                        for app_item in apps {
                            Frame::group(ui.style())
                                .fill(app.theme.subpanel_fill())
                                .stroke(Stroke::new(1.0, app.theme.accent().gamma_multiply(0.30)))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(&app_item.name).strong());
                                    ui.small(&app_item.description);
                                    ui.monospace(&app_item.command);
                                    ui.horizontal(|ui| {
                                        if ui.button(app.tr("载入表单", "Load into form")).clicked()
                                        {
                                            app.app_form = AppForm {
                                                name: app_item.name.clone(),
                                                command: app_item.command.clone(),
                                                description: app_item.description.clone(),
                                            };
                                        }
                                        if ui.button(app.tr("删除", "Remove")).clicked() {
                                            app.remove_app(&app_item.name);
                                        }
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    }
                },
            );
        });
    });
}

pub(super) fn render_sessions_tab_v3(app: &mut ClawGuiApp, ctx: &egui::Context) {
    let sessions = app.sessions.clone();
    egui::CentralPanel::default().show(ctx, |ui| {
        section_card(
            ui,
            app.theme,
            app.tr("工作区会话", "Workspace Sessions"),
            |ui| {
                ui.horizontal(|ui| {
                    if ui.button(app.tr("刷新", "Refresh")).clicked() {
                        app.refresh_session_list();
                    }
                    ui.label(format!("{}: {}", app.tr("总数", "Total"), sessions.len()));
                });

                if sessions.is_empty() {
                    ui.label(app.tr(
                        "当前工作区还没有已保存会话。",
                        "No saved sessions in this workspace yet.",
                    ));
                    return;
                }

                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for summary in sessions {
                            let active = app.active_session_path == summary.path;
                            Frame::group(ui.style())
                                .fill(app.theme.subpanel_fill())
                                .stroke(Stroke::new(1.0, app.theme.accent().gamma_multiply(0.30)))
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(&summary.id).strong().color(
                                            if active {
                                                app.theme.accent()
                                            } else {
                                                ui.visuals().text_color()
                                            },
                                        ));
                                        if active {
                                            ui.label(app.tr("当前会话", "Current session"));
                                        }
                                    });
                                    ui.small(format!(
                                        "{}: {}",
                                        app.tr("消息数", "Messages"),
                                        summary.message_count
                                    ));
                                    if let Some(parent) = &summary.parent_session_id {
                                        ui.small(format!(
                                            "{}: {}",
                                            app.tr("父会话", "Parent"),
                                            parent
                                        ));
                                    }
                                    if let Some(branch) = &summary.branch_name {
                                        ui.small(format!(
                                            "{}: {}",
                                            app.tr("分支", "Branch"),
                                            branch
                                        ));
                                    }
                                    ui.small(summary.path.display().to_string());
                                    ui.horizontal(|ui| {
                                        if ui
                                            .button(app.tr("载入到聊天", "Load into chat"))
                                            .clicked()
                                        {
                                            app.load_session_summary_action(&summary);
                                        }
                                    });
                                });
                            ui.add_space(8.0);
                        }
                    });
            },
        );
    });
}

pub(super) fn render_thread_window_v4(app: &mut ClawGuiApp, ctx: &egui::Context) {
    if !app.show_thread_form {
        return;
    }

    let mut open = app.show_thread_form;
    let mut create = false;
    let mut pick_folder = false;
    let mut cancel = false;

    egui::Window::new(app.tr("新建线程", "Create Thread"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .show(ctx, |ui| {
            ui.label(app.tr("线程名称", "Thread name"));
            ui.text_edit_singleline(&mut app.thread_form.name);
            ui.label(app.tr("文件夹", "Folder"));
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut app.thread_form.folder)
                        .desired_width(ui.available_width() - 90.0),
                );
                if ui.button(app.tr("选择", "Choose")).clicked() {
                    pick_folder = true;
                }
            });
            ui.label(app.tr("说明", "Description"));
            ui.add(TextEdit::multiline(&mut app.thread_form.description).desired_rows(3));
            ui.horizontal(|ui| {
                if ui.button(app.tr("创建并进入", "Create & Open")).clicked() {
                    create = true;
                }
                if ui.button(app.tr("取消", "Cancel")).clicked() {
                    cancel = true;
                }
            });
        });

    app.show_thread_form = open;
    if cancel {
        app.show_thread_form = false;
    }
    if pick_folder {
        // 统一沿用现有目录选择逻辑，避免线程弹窗和侧边栏出现行为分叉。
        app.import_folder_picker();
    }
    if create {
        app.add_thread_action();
    }
}

pub(super) fn render_model_quick_window_v1(app: &mut ClawGuiApp, ctx: &egui::Context) {
    if !app.show_model_quick_form {
        return;
    }

    let mut open = app.show_model_quick_form;
    let mut save = false;
    let mut save_activate = false;

    egui::Window::new(app.tr("新建模型", "New Model"))
        .open(&mut open)
        .collapsible(false)
        .default_width(500.0)
        .show(ctx, |ui| {
            ui.label(app.tr("名称", "Name"));
            ui.text_edit_singleline(&mut app.llm_form.name);
            ui.label(app.tr("Provider", "Provider"));
            ui.text_edit_singleline(&mut app.llm_form.provider);
            ui.label(app.tr("Model", "Model"));
            ui.text_edit_singleline(&mut app.llm_form.model);
            ui.label(app.tr("Base URL", "Base URL"));
            ui.text_edit_singleline(&mut app.llm_form.base_url);
            let mut key_from_env = app.llm_key_from_env;
            ui.label(app.tr("Key 模式", "Key mode"));
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut key_from_env,
                    false,
                    app.tr("直接填入 Key", "Inline key"),
                );
                ui.selectable_value(&mut key_from_env, true, app.tr("环境变量", "Env var"));
            });
            app.llm_key_from_env = key_from_env;

            if app.llm_form.api_key_env.trim().is_empty() {
                app.llm_form.api_key_env = app.derive_env_var_name();
            }
            ui.label(app.tr("环境变量名", "Environment variable"));
            ui.text_edit_singleline(&mut app.llm_form.api_key_env);
            if app.llm_key_from_env {
                ui.small(app.tr(
                    "当前 profile 会优先从环境变量读取 Key。",
                    "This profile will prefer reading API key from env variable.",
                ));
            }

            ui.label(app.tr("API Key", "API key"));
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut app.llm_form.api_key)
                        .password(!app.show_api_key)
                        .desired_width(ui.available_width() - 90.0),
                );
                if ui
                    .button(app.tr(
                        if app.show_api_key { "隐藏" } else { "显示" },
                        if app.show_api_key { "Hide" } else { "Show" },
                    ))
                    .clicked()
                {
                    app.show_api_key = !app.show_api_key;
                }
            });
            ui.horizontal(|ui| {
                if ui.button(app.tr("写入当前环境", "Write env")).clicked() {
                    app.write_api_key_to_env_action(false);
                }
                if ui
                    .button(app.tr("写入并持久化", "Write + persist"))
                    .clicked()
                {
                    app.write_api_key_to_env_action(true);
                }
            });

            ui.horizontal(|ui| {
                if ui.button(app.tr("保存", "Save")).clicked() {
                    save = true;
                }
                if ui.button(app.tr("保存并激活", "Save + Activate")).clicked() {
                    save_activate = true;
                }
            });
        });

    app.show_model_quick_form = open;
    if save {
        app.save_profile_action();
    }
    if save_activate {
        let profile_name = app.llm_form.name.trim().to_string();
        if app.save_profile_action() {
            app.switch_profile_action(Some(&profile_name));
            app.show_model_quick_form = false;
        }
    }
}
