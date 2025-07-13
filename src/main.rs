use axum::{
    extract::{Multipart, Path, State, ConnectInfo, DefaultBodyLimit},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};
use std::{sync::Arc, time::Duration, net::SocketAddr};
use tower_http::{
    compression::CompressionLayer,
    services::ServeDir,
    cors::CorsLayer,
    timeout::TimeoutLayer,
};
use dashmap::DashMap;
use tokio::fs;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use tracing_subscriber;

// Cache untuk performa
type Cache = Arc<DashMap<String, CacheEntry>>;

// Rate limiting untuk likes
type RateLimiter = Arc<DashMap<String, DateTime<Utc>>>;

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
    rate_limiter: RateLimiter,
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
    
    // Setup rate limiter
    println!("🔧 DEBUG: Setting up rate limiter...");
    let rate_limiter = Arc::new(DashMap::new());
    println!("✅ DEBUG: Rate limiter setup completed");
    
    let state = AppState { db, cache, rate_limiter };

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
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024)) // 100MB limit
        .layer(TimeoutLayer::new(Duration::from_secs(300))) // 5 minute timeout
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
    <title>🔥 MalFunc - CTF Writeup 🔥</title>
    <style>
        body { background: #000; color: #00ff00; font-family: 'Courier New', monospace; margin: 0; padding: 20px; }
        .container { max-width: 900px; margin: 0 auto; background: rgba(0, 0, 0, 0.8); border: 2px solid #00ff00; border-radius: 10px; padding: 20px; }
        h1 { text-align: center; color: #ff0080; font-size: 2.5em; text-shadow: 0 0 10px #ff0080; }
        .nav { text-align: center; margin-bottom: 30px; padding: 10px; }
        .nav a { color: #ffff00; text-decoration: none; margin: 0 15px; padding: 8px 15px; border: 1px solid #ffff00; border-radius: 5px; transition: all 0.3s; }
        .nav a:hover { background: #ffff00; color: #000; }
        .portfolio-item { 
            border: 2px solid #00ff00; 
            margin: 25px 0; 
            padding: 20px; 
            border-radius: 10px;
            background: linear-gradient(135deg, rgba(0, 255, 0, 0.1), rgba(0, 255, 255, 0.05));
            box-shadow: 0 4px 15px rgba(0, 255, 0, 0.3);
        }
        .portfolio-title { 
            color: #ffff00; 
            font-size: 1.6em; 
            font-weight: bold; 
            margin-bottom: 15px;
            text-shadow: 0 0 5px #ffff00;
        }
        .portfolio-desc { 
            color: #00ffff; 
            margin: 15px 0; 
            line-height: 1.6;
            font-size: 1.1em;
            white-space: pre-wrap;
        }
        .stats-row { 
            margin: 15px 0; 
            padding: 10px; 
            background: rgba(0, 0, 0, 0.4); 
            border-radius: 5px;
            display: flex;
            justify-content: space-between;
            align-items: center;
            flex-wrap: wrap;
        }
        .stats-left { display: flex; gap: 20px; }
        .stat-item { color: #ff0080; font-weight: bold; }
        .date-item { color: #ff0080; font-style: italic; }
        .actions-row { 
            margin: 15px 0; 
            display: flex; 
            gap: 10px; 
            justify-content: center;
            flex-wrap: wrap;
        }
        .download-btn { 
            background: linear-gradient(45deg, #ff0080, #00ff00); 
            border: none; 
            color: #000; 
            padding: 12px 20px; 
            text-decoration: none; 
            font-weight: bold; 
            border-radius: 8px;
            cursor: pointer;
            transition: all 0.3s;
            font-family: 'Courier New', monospace;
            text-transform: uppercase;
            letter-spacing: 1px;
        }
        .download-btn:hover { 
            transform: translateY(-2px); 
            box-shadow: 0 4px 15px rgba(255, 0, 128, 0.4);
        }
        .view-btn {
            background: linear-gradient(45deg, #00bfff, #87ceeb);
            color: #000;
        }
        .view-btn:hover {
            box-shadow: 0 4px 15px rgba(0, 191, 255, 0.4);
        }
        .download-btn-green {
            background: linear-gradient(45deg, #00ff00, #32cd32);
            color: #000;
        }
        .download-btn-green:hover {
            box-shadow: 0 4px 15px rgba(0, 255, 0, 0.4);
        }
        .like-btn {
            background: linear-gradient(45deg, #ff1020, #ff0080);
        }
        .like-btn:hover {
            box-shadow: 0 4px 15px rgba(255, 16, 32, 0.4);
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🔥 MALFUNC WRITEUP 🔥</h1>
        <div class="nav">
            <a href="/">🏠 HOME</a>
            <a href="/login">🔐 ADMIN</a>
        </div>
"#);

    if portfolios.is_empty() {
        html.push_str(r#"
        <div style="text-align: center; color: #ff0080; font-size: 1.5em; margin: 50px 0;">
            <p>📂 NO CTF WRITEUPS YET</p>
        </div>
        "#);
    } else {
        for portfolio in &portfolios {
            html.push_str(&format!(r#"
        <div class="portfolio-item">
            <div class="portfolio-title">🎯 {}</div>
            <div class="portfolio-desc">{}</div>
            <div class="stats-row">
                <div class="stats-left">
                    <span class="stat-item">❤️ {} likes</span>
                    <span class="stat-item">👁️ {} views</span>
                </div>
                <div class="date-item">📅 {}</div>
            </div>
            <div class="actions-row">
                <a href="/view/{}" class="download-btn view-btn">👁️ VIEW PDF</a>
                <a href="/download/{}" class="download-btn download-btn-green">📥 DOWNLOAD</a>
                <button onclick="likePortfolio('{}')" class="download-btn like-btn">❤️ LIKE</button>
            </div>
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
                } else if (response.status === 429) {
                    alert('⏰ Please wait 5 minutes before liking again!');
                } else {
                    alert('❌ Error liking writeup');
                }
            } catch (error) {
                console.error('Network error:', error);
                alert('🌐 Network error - please try again');
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
    <title>🔐 Admin Login - MalFunc Writeup</title>
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
            <a href="/">← Back to Writeup</a>
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
    <title>🔐 Admin Login - MalFunc Writeup</title>
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
            <a href="/">← Back to Writeup</a>
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
    <title>🔧 Admin Panel - MalFunc Writeup</title>
    <style>
        body { background: #000; color: #00ff00; font-family: 'Courier New', monospace; margin: 0; padding: 20px; }
        .container { max-width: 1100px; margin: 0 auto; background: rgba(0, 0, 0, 0.8); border: 2px solid #00ff00; border-radius: 10px; padding: 20px; }
        h1 { text-align: center; color: #ff0080; font-size: 2.5em; text-shadow: 0 0 10px #ff0080; }
        .nav { text-align: center; margin-bottom: 30px; padding: 10px; }
        .nav a { color: #ffff00; text-decoration: none; margin: 0 15px; padding: 8px 15px; border: 1px solid #ffff00; border-radius: 5px; transition: all 0.3s; }
        .nav a:hover { background: #ffff00; color: #000; }
        .portfolio-item { 
            border: 2px solid #00ff00; 
            margin: 25px 0; 
            padding: 20px; 
            border-radius: 10px;
            background: linear-gradient(135deg, rgba(0, 255, 0, 0.1), rgba(0, 255, 255, 0.05));
            box-shadow: 0 4px 15px rgba(0, 255, 0, 0.3);
        }
        .portfolio-title { 
            color: #ffff00; 
            font-size: 1.6em; 
            font-weight: bold; 
            margin-bottom: 15px;
            text-shadow: 0 0 5px #ffff00;
        }
        .portfolio-desc { 
            color: #00ffff; 
            margin: 15px 0; 
            line-height: 1.6;
            font-size: 1.1em;
            white-space: pre-wrap;
        }
        .admin-stats { 
            margin: 15px 0; 
            padding: 15px; 
            background: rgba(0, 0, 0, 0.4); 
            border-radius: 8px;
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
            gap: 15px;
        }
        .stat-item { color: #ff0080; font-weight: bold; }
        .delete-btn { 
            background: #ff1020; 
            border: none; 
            color: #fff; 
            padding: 8px 15px; 
            font-family: 'Courier New', monospace; 
            cursor: pointer; 
            border-radius: 5px;
            transition: all 0.3s;
            font-weight: bold;
        }
        .delete-btn:hover { 
            background: #ff3040; 
            transform: translateY(-2px);
        }
        .admin-actions {
            margin-top: 15px;
            text-align: right;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🔧 ADMIN PANEL</h1>
        <div class="nav">
            <a href="/">🏠 HOME</a>
            <a href="/admin/add">➕ ADD WRITEUP</a>
            <form method="post" action="/logout" style="display: inline;">
                <button type="submit" style="background: #ff1020; border: none; color: #fff; padding: 5px 10px; font-family: 'Courier New', monospace; cursor: pointer;">🚪 LOGOUT</button>
            </form>
        </div>
"#);

    if portfolios.is_empty() {
        html.push_str(r#"
        <div style="text-align: center; color: #ff0080; font-size: 1.5em; margin: 50px 0;">
            <p>📂 NO WRITEUPS YET</p>
            <p><a href="/admin/add" style="color: #ffff00;">➕ ADD YOUR FIRST WRITEUP</a></p>
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
    <title>➕ Add Writeup - MalFunc Admin</title>
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
        <h1>➕ ADD NEW WRITEUP</h1>
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

    println!("🔧 DEBUG: Starting multipart processing...");

    // Use a more careful approach to handle multipart
    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => {
                let name = match field.name() {
                    Some(name) => name.to_string(),
                    None => {
                        println!("⚠️ DEBUG: Field with no name, skipping...");
                        // Try to consume the field anyway
                        let _ = field.bytes().await;
                        continue;
                    }
                };
                
                println!("🔧 DEBUG: Processing field: {}", name);
                
                match name.as_str() {
                    "title" => {
                        match field.text().await {
                            Ok(text) => {
                                title = text;
                                println!("✅ DEBUG: Title received: {}", title);
                            },
                            Err(e) => {
                                println!("❌ DEBUG: Error reading title: {}", e);
                                return Html("Error reading title").into_response();
                            }
                        }
                    }
                    "description" => {
                        match field.text().await {
                            Ok(text) => {
                                description = text;
                                println!("✅ DEBUG: Description received ({} chars)", description.len());
                            },
                            Err(e) => {
                                println!("❌ DEBUG: Error reading description: {}", e);
                                return Html("Error reading description").into_response();
                            }
                        }
                    }
                    "pdf" => {
                        if let Some(filename) = field.file_name() {
                            let filename = filename.to_string();
                            println!("🔧 DEBUG: Processing PDF file: {}", filename);
                            
                            match field.bytes().await {
                                Ok(data) => {
                                    println!("✅ DEBUG: PDF data received ({} bytes)", data.len());
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
                                Err(e) => {
                                    println!("❌ DEBUG: Error reading PDF bytes: {}", e);
                                    return Html("Error reading PDF file").into_response();
                                }
                            }
                        } else {
                            // No filename, consume the field anyway
                            let _ = field.bytes().await;
                            println!("⚠️ DEBUG: PDF field with no filename");
                        }
                    }
                    _ => {
                        // Skip unknown fields
                        println!("⚠️ DEBUG: Unknown field '{}', skipping...", name);
                        match field.bytes().await {
                            Ok(_) => {},
                            Err(e) => {
                                println!("❌ DEBUG: Error skipping field {}: {}", name, e);
                                // Don't return error for unknown fields, just continue
                            }
                        }
                    }
                }
            }
            Ok(None) => {
                println!("✅ DEBUG: Finished processing all fields");
                break;
            }
            Err(e) => {
                println!("❌ DEBUG: Error getting next field: {}", e);
                return Html(format!("Error processing upload: {}", e)).into_response();
            }
        }
    }

    println!("🔧 DEBUG: Validation - title: '{}', desc length: {}, pdf: '{}'", 
             title, description.len(), pdf_filename);

    if !title.is_empty() && !description.is_empty() && !pdf_filename.is_empty() {
        let id = Uuid::new_v4().to_string();
        
        match sqlx::query(
            "INSERT INTO portfolios (id, title, description, pdf_filename) VALUES (?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&title)
        .bind(&description)
        .bind(&pdf_filename)
        .execute(&state.db)
        .await {
            Ok(_) => {
                println!("✅ DEBUG: Portfolio added successfully with ID: {}", id);
                // Clear cache
                state.cache.remove("index");
            }
            Err(e) => {
                println!("❌ DEBUG: Database error: {}", e);
                return Html("Error saving to database").into_response();
            }
        }
    } else {
        println!("❌ DEBUG: Missing required fields - title: '{}', desc: '{}', pdf: '{}'", 
                 title, description, pdf_filename);
        return Html("Missing required fields. Please fill all fields and select a PDF file.").into_response();
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let client_ip = addr.ip().to_string();
    let rate_key = format!("{}:{}", client_ip, id);
    
    // Check rate limiting (5 menit = 300 detik)
    if let Some(last_like_time) = state.rate_limiter.get(&rate_key) {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(*last_like_time);
        if elapsed.num_seconds() < 300 { // 5 menit
            return (StatusCode::TOO_MANY_REQUESTS, "Rate limited: Wait 5 minutes before liking again").into_response();
        }
    }
    
    // Increment like count
    let result = sqlx::query("UPDATE portfolios SET likes = likes + 1 WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => {
            // Update rate limiter
            state.rate_limiter.insert(rate_key, Utc::now());
            
            // Clear cache since like count changed
            state.cache.remove("index");
            
            StatusCode::OK.into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
