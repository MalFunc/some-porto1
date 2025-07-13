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
    likes: i64,
    views: i64,
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
    println!("🚀 DEBUG: Starting CTF Portfolio application...");

    // Setup database
    println!("🔧 DEBUG: Setting up database...");
    let db = setup_database().await?;
    println!("✅ DEBUG: Database setup completed");
    
    // Setup cache
    println!("🔧 DEBUG: Setting up cache...");
    let cache = Arc::new(DashMap::new());
    println!("✅ DEBUG: Cache setup completed");
    
    let state = AppState { db, cache };

    // Ensure upload directory exists
    println!("🔧 DEBUG: Creating upload directories...");
    match fs::create_dir_all("uploads").await {
        Ok(_) => println!("✅ DEBUG: uploads directory created/exists"),
        Err(e) => {
            println!("❌ DEBUG: uploads directory error: {}", e);
            // Don't fail here, just log the error
        }
    }
    match fs::create_dir_all("static").await {
        Ok(_) => println!("✅ DEBUG: static directory created/exists"),  
        Err(e) => {
            println!("❌ DEBUG: static directory error: {}", e);
            // Don't fail here, just log the error
        }
    }

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health_check))
        .route("/admin", get(admin_page))
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .route("/admin/add", get(add_portfolio_page).post(add_portfolio))
        .route("/admin/delete/:id", post(delete_portfolio))
        .route("/download/:filename", get(download_pdf))
        .route("/view/:id", get(view_pdf))
        .route("/like/:id", post(like_portfolio))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/uploads", ServeDir::new("uploads"))
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("🚀 Server running on http://0.0.0.0:3000");
    println!("✅ DEBUG: Application fully initialized, starting web server...");
    
    axum::serve(listener, app).await?;
    Ok(())
}

async fn setup_database() -> anyhow::Result<SqlitePool> {
    // Use in-memory database for now to avoid permission issues
    let db_url = "sqlite::memory:";
    println!("🔧 DEBUG: Connecting to database: {}", db_url);
    let db = SqlitePool::connect(db_url).await?;
    println!("✅ DEBUG: Database connected successfully");
    
    // Create table manually since SQLx 0.6 doesn't have migrate! macro
    println!("🔧 DEBUG: Creating tables...");
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS portfolios (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            pdf_filename TEXT NOT NULL,
            likes INTEGER DEFAULT 0,
            views INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&db)
    .await?;
    println!("✅ DEBUG: Table created successfully");

    // Create index for better performance
    println!("🔧 DEBUG: Creating index...");
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_portfolios_created_at ON portfolios(created_at DESC)")
        .execute(&db)
        .await?;
    println!("✅ DEBUG: Index created successfully");

    Ok(db)
}

async fn health_check() -> impl IntoResponse {
    println!("🩺 DEBUG: Health check called");
    "OK"
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
        "SELECT id, title, description, pdf_filename, likes, views, created_at FROM portfolios ORDER BY created_at DESC"
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
            <div style="margin: 15px 0;">
                <span style="color: #ff0080;">❤️ {} likes</span>
                <span style="color: #00ffff; margin-left: 15px;">👁️ {} views</span>
                <span style="color: #ff0080; float: right;">📅 {}</span>
            </div>
            <div style="margin: 10px 0;">
                <a href="/view/{}" class="download-btn" style="margin-right: 10px;">👁️ VIEW PDF</a>
                <a href="/download/{}" class="download-btn" style="margin-right: 10px;">📥 DOWNLOAD</a>
                <button onclick="likePortfolio('{}')" class="download-btn" style="background: linear-gradient(45deg, #ff1020, #ff0080);">❤️ LIKE</button>
            </div>
            <div style="clear: both;"></div>
        </div>
            "#, portfolio.title, portfolio.description, portfolio.likes, portfolio.views, portfolio.created_at.format("%Y-%m-%d"), portfolio.id, portfolio.pdf_filename, portfolio.id));
        }
    }

    html.push_str(r#"
    </div>
    <script>
        async function likePortfolio(id) {
            try {
                const response = await fetch(`/like/${id}`, { method: 'POST' });
                if (response.ok) {
                    location.reload(); // Refresh page to show new like count
                } else {
                    alert('Error liking portfolio');
                }
            } catch (error) {
                alert('Network error');
            }
        }
    </script>
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
    if form.username == "admin" && form.password == "maliksigma" {
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
    Redirect::to("/").into_response()
}

async fn admin_page(State(state): State<AppState>) -> impl IntoResponse {
    let portfolios = sqlx::query_as::<_, Portfolio>(
        "SELECT id, title, description, pdf_filename, likes, views, created_at FROM portfolios ORDER BY created_at DESC"
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
            <div style="margin: 10px 0;">
                <span style="color: #ff0080;">❤️ {} likes</span>
                <span style="color: #00ffff; margin-left: 15px;">�️ {} views</span>
                <span style="color: #ff0080; margin-left: 15px;">�📅 {}</span>
                <span style="color: #00ffff; margin-left: 15px;">📄 {}</span>
                <form method="post" action="/admin/delete/{}" style="display: inline; float: right;">
                    <button type="submit" class="delete-btn" onclick="return confirm('Are you sure?')">🗑️ DELETE</button>
                </form>
            </div>
            <div style="clear: both;"></div>
        </div>
            "#, portfolio.title, portfolio.description, portfolio.likes, portfolio.views, portfolio.created_at.format("%Y-%m-%d"), portfolio.pdf_filename, portfolio.id));
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
            "pdf" => {
                if let Some(filename) = field.file_name() {
                    let filename = filename.to_string(); // Clone filename first
                    let data = field.bytes().await.unwrap();
                    let uuid = Uuid::new_v4().to_string();
                    pdf_filename = format!("{}_{}", uuid, filename);
                    
                    println!("🔧 DEBUG: Writing file to uploads/{}", pdf_filename);
                    match fs::write(format!("uploads/{}", pdf_filename), data).await {
                        Ok(_) => println!("✅ DEBUG: File written successfully"),
                        Err(e) => {
                            println!("❌ DEBUG: File write error: {}", e);
                            return Html("Error uploading file").into_response();
                        }
                    }
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

    Redirect::to("/admin").into_response()
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

    Redirect::to("/admin").into_response()
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

async fn view_pdf(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Increment view count
    let _ = sqlx::query("UPDATE portfolios SET views = views + 1 WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await;

    // Clear cache since view count changed
    state.cache.remove("index");

    // Get portfolio info
    if let Ok(row) = sqlx::query("SELECT pdf_filename, title FROM portfolios WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await
    {
        let filename: String = row.get("pdf_filename");
        let title: String = row.get("title");
        
        match fs::read(format!("uploads/{}", filename)).await {
            Ok(data) => {
                let mut headers = HeaderMap::new();
                headers.insert("Content-Type", "application/pdf".parse().unwrap());
                headers.insert(
                    "Content-Disposition",
                    format!("inline; filename=\"{}\"", filename).parse().unwrap(),
                );
                (StatusCode::OK, headers, data).into_response()
            }
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        }
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn like_portfolio(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Increment like count
    let result = sqlx::query("UPDATE portfolios SET likes = likes + 1 WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await;

    // Clear cache since like count changed
    state.cache.remove("index");

    match result {
        Ok(_) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
