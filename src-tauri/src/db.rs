use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tauri::{AppHandle, Manager};

pub async fn init_db(app: &AppHandle) -> Result<SqlitePool, sqlx::Error> {
    // Resolve a path inside the app's data directory, e.g.:
    // C:\Users\<user>\AppData\Roaming\grimoire\grimoire.db
    let app_dir = app
        .path()
        .app_data_dir()
        .expect("could not resolve app data directory");

    std::fs::create_dir_all(&app_dir).expect("could not create app data directory");

    let db_path = app_dir.join("grimoire.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    // Run any pending migrations from the migrations/ folder.
    // sqlx tracks which ones have already been applied, so this is safe to call every startup.
    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}
