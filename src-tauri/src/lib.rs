pub mod archive;
pub mod commands;
pub mod connections;
pub mod crypto;
pub mod files;
#[cfg(test)]
mod import_compatibility_tests;
pub mod imports;
#[cfg(test)]
mod integration_tests;
pub mod integrations;
pub mod keys;
pub mod metrics;
pub mod model;
pub mod operations;
pub mod platform;
pub mod remember;
pub mod shared_terminal;
pub mod ssh_compat;
pub mod state;
pub mod sync;
pub mod transfers;
pub mod tunnels;
pub mod updater;
pub mod vault;
pub fn run() {
    use commands::*;
    use tauri::Manager;
    let shutting_down = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let builder = tauri::Builder::default();
    #[cfg(all(feature = "e2e", debug_assertions))]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .manage(std::sync::Arc::new(state::AppState::default()))
        .manage(updater::UpdateState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            updater::update_info,
            updater::update_check,
            updater::update_install,
            app_info,
            vault_create,
            vault_open,
            vault_open_remembered,
            vault_remember,
            vault_info,
            vault_lock,
            records_save,
            records_delete,
            vault_copy,
            backup_create,
            backup_bundle,
            csv_headers,
            backup_preview,
            backup_restore,
            import_preview,
            import_apply,
            operation_cancel,
            transfer_control,
            export_preview,
            export_write,
            key_import,
            key_generate,
            agent_identities,
            key_install,
            sync_status,
            sync_preview,
            sync_bind,
            session_start,
            session_input,
            session_metrics_set,
            prompt_answer,
            sftp_connect,
            sftp_disconnect,
            file_list,
            file_action,
            file_edit,
            file_edit_save,
            file_transfer,
            tunnel_start,
            tunnel_stop,
            sync_test,
            sync_watch,
            sync_prepare,
            sync_list,
            sync_upload,
            sync_run,
            sync_resolve,
            sync_download,
            team_list,
            team_set,
            share_list,
            share_start,
            share_control,
            serial_ports,
            lab_profile,
            cloud_discover,
            api_bridge
        ])
        .build(tauri::generate_context!())
        .expect("TermTerm startup failed")
        .run(move |app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if !shutting_down.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    api.prevent_exit();
                    let app = app.clone();
                    let state = app.state::<state::Shared>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = tokio::time::timeout(
                            std::time::Duration::from_secs(8),
                            commands::close_connections(&state),
                        )
                        .await;
                        app.exit(0);
                    });
                }
            }
        });
}
