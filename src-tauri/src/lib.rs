mod commands;
mod db;
mod vector;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let pool = db::init_db(&app_handle)
                    .await
                    .expect("failed to initialise database");
                app_handle.manage(pool);

                let vdb = vector::init(&app_handle)
                    .await
                    .expect("failed to initialise vector database");
                app_handle.manage(vector::VectorDb(vdb));
            });

            Ok(())
        });

    // Register commands — debug-only commands are excluded from release builds.
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::create_note,
        commands::get_note,
        commands::list_notes,
        commands::update_note,
        commands::move_note,
        commands::delete_note,
        commands::create_folder,
        commands::list_folders,
        commands::rename_folder,
        commands::delete_folder,
        commands::chat,
        commands::index_note,
        commands::remove_note_index,
        commands::search_notes,
        commands::reindex_all,
        commands::sync_note_relations,
        commands::get_note_tags,
        commands::get_note_links,
        commands::get_backlinks,
        commands::list_notes_by_tag,
        commands::list_all_tags,
        commands::get_graph_data,
        commands::debug_search,
        commands::seed_notes,
    ]);

    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::create_note,
        commands::get_note,
        commands::list_notes,
        commands::update_note,
        commands::move_note,
        commands::delete_note,
        commands::create_folder,
        commands::list_folders,
        commands::rename_folder,
        commands::delete_folder,
        commands::chat,
        commands::index_note,
        commands::remove_note_index,
        commands::search_notes,
        commands::reindex_all,
        commands::sync_note_relations,
        commands::get_note_tags,
        commands::get_note_links,
        commands::get_backlinks,
        commands::list_notes_by_tag,
        commands::list_all_tags,
        commands::get_graph_data,
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
