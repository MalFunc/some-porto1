use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};
use std::{sync::Arc, time::Duration};
use tower_http::{
    compression::CompressionLayer,
    services::ServeDir,
    cors::CorsLayer,
};
use dashmap::DashMap;
use tokio::fs;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use tracing_subscriber;

// Cache untuk performa
type Cache = Arc<DashMap<String, CacheEntry>>;

#[derive(Clone)]
struct CacheEntry {
    data: String,
    timestamp: DateTime<Utc>,
    ttl: Duration,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(self.timestamp);
        elapsed.num_seconds() as u64 > self.ttl.as_secs()
    }
}

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    cache: Cache,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
struct Portfolio {
    id: String,
    title: String,
    description: String,
    pdf_filename: String,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct PortfolioForm {
    title: String,
    description: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Setup database
    let db = setup_database().await?;
    
    // Setup cache
    let cache = Arc::new(DashMap::new());
    
    let state = AppState { db, cache };

    // Ensure upload directory exists
    fs::create_dir_all("uploads").await?;
    fs::create_dir_all("static").await?;

    let app = Router::new()
        .route("/", get(index))
        .route("/admin", get(admin_page))
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .route("/admin/add", get(add_portfolio_page).post(add_portfolio))
        .route("/admin/delete/:id", post(delete_portfolio))
        .route("/download/:filename", get(download_pdf))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/uploads", ServeDir::new("uploads"))
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("🚀 Server running on http://0.0.0.0:3000");
    
    axum::serve(listener, app).await?;
    Ok(())
}

async fn setup_database() -> anyhow::Result<SqlitePool> {
    let db = SqlitePool::connect("sqlite:database.db").await?;
    
    // Create table manually since SQLx 0.6 doesn't have migrate! macro
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS portfolios (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            pdf_filename TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&db)
    .await?;

    // Create index for better performance
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_portfolios_created_at ON portfolios(created_at DESC)")
        .execute(&db)
        .await?;

    Ok(db)
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    // Check cache first
    if let Some(cached) = state.cache.get("index") {
        if !cached.is_expired() {
            return Html(cached.data.clone());
        } else {
            state.cache.remove("index");
        }
    }

    let portfolios = sqlx::query_as::<_, Portfolio>(
        "SELECT id, title, description, pdf_filename, created_at FROM portfolios ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    // Simple HTML template without Askama
    let mut html = String::from(r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>🔥 CTF Portfolio - Hacker Zone 🔥</title>
    <style>
        body { background: #000; color: #00ff00; font-family: 'Courier New', monospace; margin: 0; padding: 20px; }
        .container { max-width: 800px; margin: 0 auto; background: rgba(0, 0, 0, 0.8); border: 2px solid #00ff00; border-radius: 10px; padding: 20px; }
        h1 { text-align: center; color: #ff0080; font-size: 2.5em; }
        .nav { text-align: center; margin-bottom: 30px; padding: 10px; }
        .nav a { color: #ffff00; text-decoration: none; margin: 0 15px; padding: 5px 10px; border: 1px solid #ffff00; }
        .portfolio-item { border: 2px solid #00ff00; margin: 20px 0; padding: 15px; }
        .portfolio-title { color: #ffff00; font-size: 1.4em; font-weight: bold; }
        .portfolio-desc { color: #00ffff; margin: 10px 0; }
        .download-btn { background: linear-gradient(45deg, #ff0080, #00ff00); border: none; color: #000; padding: 8px 15px; text-decoration: none; font-weight: bold; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🔥 CTF PORTFOLIO 🔥</h1>
        <div class="nav">
            <a href="/">🏠 HOME</a>
            <a href="/login">🔐 ADMIN</a>
        </div>
"#);

    if portfolios.is_empty() {
        html.push_str(r#"
        <div style="text-align: center; color: #ff0080; font-size: 1.5em; margin: 50px 0;">
            <p>📂 NO CTF PORTFOLIOS YET</p>
        </div>
        "#);
    } else {
        for portfolio in &portfolios {
            html.push_str(&format!(r#"
        <div class="portfolio-item">
            <div class="portfolio-title">🎯 {}</div>
            <div class="portfolio-desc">{}</div>
            <div>
                <a href="/download/{}" class="download-btn">📥 DOWNLOAD PDF</a>
                <span style="color: #ff0080; float: right;">📅 {}</span>
            </div>
            <div style="clear: both;"></div>
        </div>
            "#, portfolio.title, portfolio.description, portfolio.pdf_filename, portfolio.created_at.format("%Y-%m-%d")));
        }
    }

    html.push_str(r#"
    </div>
</body>
</html>
    "#);
    
    // Cache the result for 5 minutes
    state.cache.insert("index".to_string(), CacheEntry {
        data: html.clone(),
        timestamp: Utc::now(),
        ttl: Duration::from_secs(300),
    });

    Html(html)
}

async fn login_page() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>🔐 Admin Login - CTF Portfolio</title>
    <style>
        body { background: #000; color: #00ff00; font-family: 'Courier New', monospace; margin: 0; padding: 20px; }
        .container { max-width: 400px; margin: 100px auto; background: rgba(0, 0, 0, 0.9); border: 2px solid #00ff00; border-radius: 10px; padding: 30px; }
        h1 { text-align: center; color: #ff0080; font-size: 2em; margin-bottom: 30px; }
        .form-group { margin: 20px 0; }
        label { display: block; color: #ffff00; margin-bottom: 5px; }
        input { width: 100%; padding: 10px; background: #111; color: #00ff00; border: 1px solid #00ff00; border-radius: 3px; font-family: 'Courier New', monospace; }
        button { width: 100%; padding: 12px; background: linear-gradient(45deg, #ff0080, #00ff00); border: none; color: #000; font-weight: bold; font-size: 1.1em; margin-top: 20px; cursor: pointer; }
        button:hover { background: linear-gradient(45deg, #00ff00, #ff0080); }
        .nav { text-align: center; margin-top: 20px; }
        .nav a { color: #00ffff; text-decoration: none; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🔐 ADMIN LOGIN</h1>
        <form method="post" action="/login">
            <div class="form-group">
                <label for="username">👤 Username:</label>
                <input type="text" id="username" name="username" required>
            </div>
            <div class="form-group">
                <label for="password">🔑 Password:</label>
                <input type="password" id="password" name="password" required>
            </div>
            <button type="submit">🚀 LOGIN</button>
        </form>
        <div class="nav">
            <a href="/">← Back to Portfolio</a>
        </div>
    </div>
</body>
</html>
    "#;
    Html(html)
}

async fn login(Form(form): Form<LoginForm>) -> impl IntoResponse {
    // Simple hardcoded admin credentials (in production, use hashed passwords)
    if form.username == "admin" && form.password == "admin123" {
        Redirect::to("/admin").into_response()
    } else {
        let html = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>🔐 Admin Login - CTF Portfolio</title>
    <style>
        body { background: #000; color: #00ff00; font-family: 'Courier New', monospace; margin: 0; padding: 20px; }
        .container { max-width: 400px; margin: 100px auto; background: rgba(0, 0, 0, 0.9); border: 2px solid #00ff00; border-radius: 10px; padding: 30px; }
        h1 { text-align: center; color: #ff0080; font-size: 2em; margin-bottom: 30px; }
        .error { background: #ff1020; color: #fff; padding: 10px; margin-bottom: 20px; border-radius: 5px; text-align: center; }
        .form-group { margin: 20px 0; }
        label { display: block; color: #ffff00; margin-bottom: 5px; }
        input { width: 100%; padding: 10px; background: #111; color: #00ff00; border: 1px solid #00ff00; border-radius: 3px; font-family: 'Courier New', monospace; }
        button { width: 100%; padding: 12px; background: linear-gradient(45deg, #ff0080, #00ff00); border: none; color: #000; font-weight: bold; font-size: 1.1em; margin-top: 20px; cursor: pointer; }
        button:hover { background: linear-gradient(45deg, #00ff00, #ff0080); }
        .nav { text-align: center; margin-top: 20px; }
        .nav a { color: #00ffff; text-decoration: none; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🔐 ADMIN LOGIN</h1>
        <div class="error">❌ Invalid credentials! Try again.</div>
        <form method="post" action="/login">
            <div class="form-group">
                <label for="username">👤 Username:</label>
                <input type="text" id="username" name="username" required>
            </div>
            <div class="form-group">
                <label for="password">🔑 Password:</label>
                <input type="password" id="password" name="password" required>
            </div>
            <button type="submit">🚀 LOGIN</button>
        </form>
        <div class="nav">
            <a href="/">← Back to Portfolio</a>
        </div>
    </div>
</body>
</html>
        "#;
        Html(html).into_response()
    }
}

async fn logout() -> impl IntoResponse {
    Redirect::to("/")
}

async fn admin_page(State(state): State<AppState>) -> impl IntoResponse {
    let portfolios = sqlx::query_as::<_, Portfolio>(
        "SELECT id, title, description, pdf_filename, created_at FROM portfolios ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut html = String::from(r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>🔧 Admin Panel - CTF Portfolio</title>
    <style>
        body { background: #000; color: #00ff00; font-family: 'Courier New', monospace; margin: 0; padding: 20px; }
        .container { max-width: 1000px; margin: 0 auto; background: rgba(0, 0, 0, 0.8); border: 2px solid #00ff00; border-radius: 10px; padding: 20px; }
        h1 { text-align: center; color: #ff0080; font-size: 2.5em; }
        .nav { text-align: center; margin-bottom: 30px; padding: 10px; }
        .nav a { color: #ffff00; text-decoration: none; margin: 0 15px; padding: 5px 10px; border: 1px solid #ffff00; border-radius: 3px; }
        .portfolio-item { border: 2px solid #00ff00; margin: 20px 0; padding: 15px; background: rgba(0, 255, 0, 0.1); }
        .portfolio-title { color: #ffff00; font-size: 1.4em; font-weight: bold; }
        .portfolio-desc { color: #00ffff; margin: 10px 0; }
        .delete-btn { background: #ff1020; border: none; color: #fff; padding: 5px 10px; font-family: 'Courier New', monospace; cursor: pointer; margin-top: 10px; }
        .delete-btn:hover { background: #ff3040; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🔧 ADMIN PANEL</h1>
        <div class="nav">
            <a href="/">🏠 HOME</a>
            <a href="/admin/add">➕ ADD PORTFOLIO</a>
            <form method="post" action="/logout" style="display: inline;">
                <button type="submit" style="background: #ff1020; border: none; color: #fff; padding: 5px 10px; font-family: 'Courier New', monospace; cursor: pointer;">🚪 LOGOUT</button>
            </form>
        </div>
"#);

    if portfolios.is_empty() {
        html.push_str(r#"
        <div style="text-align: center; color: #ff0080; font-size: 1.5em; margin: 50px 0;">
            <p>📂 NO PORTFOLIOS YET</p>
            <p><a href="/admin/add" style="color: #ffff00;">➕ ADD YOUR FIRST PORTFOLIO</a></p>
        </div>
        "#);
    } else {
        for portfolio in &portfolios {
            html.push_str(&format!(r#"
        <div class="portfolio-item">
            <div class="portfolio-title">🎯 {}</div>
            <div class="portfolio-desc">{}</div>
            <div>
                <span style="color: #ff0080;">📅 {}</span>
                <span style="color: #00ffff; margin-left: 20px;">📄 {}</span>
                <form method="post" action="/admin/delete/{}" style="display: inline; float: right;">
                    <button type="submit" class="delete-btn" onclick="return confirm('Are you sure?')">🗑️ DELETE</button>
                </form>
            </div>
            <div style="clear: both;"></div>
        </div>
            "#, portfolio.title, portfolio.description, portfolio.created_at.format("%Y-%m-%d"), portfolio.pdf_filename, portfolio.id));
        }
    }

    html.push_str(r#"
    </div>
</body>
</html>
    "#);

    Html(html)
}

async fn add_portfolio_page() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>➕ Add Portfolio - CTF Admin</title>
    <style>
        body { background: #000; color: #00ff00; font-family: 'Courier New', monospace; margin: 0; padding: 20px; }
        .container { max-width: 600px; margin: 50px auto; background: rgba(0, 0, 0, 0.9); border: 2px solid #00ff00; border-radius: 10px; padding: 30px; }
        h1 { text-align: center; color: #ff0080; font-size: 2em; margin-bottom: 30px; }
        .form-group { margin: 20px 0; }
        label { display: block; color: #ffff00; margin-bottom: 5px; font-weight: bold; }
        input, textarea { width: 100%; padding: 10px; background: #111; color: #00ff00; border: 1px solid #00ff00; border-radius: 3px; font-family: 'Courier New', monospace; }
        textarea { height: 100px; resize: vertical; }
        input[type="file"] { background: #222; padding: 15px; }
        button { width: 100%; padding: 12px; background: linear-gradient(45deg, #ff0080, #00ff00); border: none; color: #000; font-weight: bold; font-size: 1.1em; margin-top: 20px; cursor: pointer; border-radius: 5px; }
        button:hover { background: linear-gradient(45deg, #00ff00, #ff0080); }
        .nav { text-align: center; margin-bottom: 20px; }
        .nav a { color: #00ffff; text-decoration: none; margin: 0 10px; }
    </style>
</head>
<body>
    <div class="container">
        <div class="nav">
            <a href="/admin">← Back to Admin</a> | 
            <a href="/">🏠 Home</a>
        </div>
        <h1>➕ ADD NEW PORTFOLIO</h1>
        <form method="post" action="/admin/add" enctype="multipart/form-data">
            <div class="form-group">
                <label for="title">🎯 Title:</label>
                <input type="text" id="title" name="title" required placeholder="Enter CTF challenge title">
            </div>
            <div class="form-group">
                <label for="description">📝 Description:</label>
                <textarea id="description" name="description" required placeholder="Describe the CTF challenge, techniques used, lessons learned..."></textarea>
            </div>
            <div class="form-group">
                <label for="pdf">📄 PDF File:</label>
                <input type="file" id="pdf" name="pdf" accept=".pdf" required>
                <small style="color: #ffff00; display: block; margin-top: 5px;">Upload your CTF writeup PDF</small>
            </div>
            <button type="submit">🚀 ADD PORTFOLIO</button>
        </form>
    </div>
</body>
</html>
    "#;
    Html(html)
}

async fn add_portfolio(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut title = String::new();
    let mut description = String::new();
    let mut pdf_filename = String::new();

    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        
        match name.as_str() {
            "title" => {
                title = field.text().await.unwrap();
            }
            "description" => {
                description = field.text().await.unwrap();
            }
            "pdf_file" => {
                if let Some(filename) = field.file_name() {
                    let filename = filename.to_string(); // Clone filename first
                    let data = field.bytes().await.unwrap();
                    let uuid = Uuid::new_v4().to_string();
                    pdf_filename = format!("{}_{}", uuid, filename);
                    
                    fs::write(format!("uploads/{}", pdf_filename), data)
                        .await
                        .unwrap();
                }
            }
            _ => {}
        }
    }

    if !title.is_empty() && !description.is_empty() && !pdf_filename.is_empty() {
        let id = Uuid::new_v4().to_string();
        
        sqlx::query(
            "INSERT INTO portfolios (id, title, description, pdf_filename) VALUES (?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&title)
        .bind(&description)
        .bind(&pdf_filename)
        .execute(&state.db)
        .await
        .unwrap();

        // Clear cache
        state.cache.remove("index");
    }

    Redirect::to("/admin")
}

async fn delete_portfolio(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Get filename first to delete file
    if let Ok(row) = sqlx::query("SELECT pdf_filename FROM portfolios WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await
    {
        let filename: String = row.get("pdf_filename");
        let _ = fs::remove_file(format!("uploads/{}", filename)).await;
    }

    sqlx::query("DELETE FROM portfolios WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .unwrap();

    // Clear cache
    state.cache.remove("index");

    Redirect::to("/admin")
}

async fn download_pdf(Path(filename): Path<String>) -> impl IntoResponse {
    match fs::read(format!("uploads/{}", filename)).await {
        Ok(data) => {
            let mut headers = HeaderMap::new();
            headers.insert("Content-Type", "application/pdf".parse().unwrap());
            headers.insert(
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", filename).parse().unwrap(),
            );
            (StatusCode::OK, headers, data).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
