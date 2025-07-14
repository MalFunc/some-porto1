use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State, Query},
    http::{HeaderMap, StatusCode, header::COOKIE},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
    middleware,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPool, Row};
use std::{sync::Arc, time::Duration};
use tower_http::{
    compression::CompressionLayer,
    services::ServeDir,
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
};
use tower::ServiceBuilder;
use dashmap::DashMap;
use tokio::fs;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use tracing_subscriber;
use anyhow::Result;

// Simple session storage (in production, use Redis or database)
type Sessions = Arc<DashMap<String, DateTime<Utc>>>;

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
    db: PgPool,
    cache: Cache,
    rate_limiter: RateLimiter,
    sessions: Sessions,
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
#[allow(dead_code)] // Form struct for potential future use
struct PortfolioForm {
    title: String,
    description: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("🚀 Starting MalFunc CTF Portfolio application...");

    // Setup database
    tracing::info!("🔧 Setting up database...");
    let db = setup_database().await?;
    tracing::info!("✅ Database setup completed");
    
    // Setup cache and rate limiter
    let cache = Arc::new(DashMap::new());
    let rate_limiter = Arc::new(DashMap::new());
    let sessions = Arc::new(DashMap::new());
    
    let state = AppState { db, cache, rate_limiter, sessions };

    // Ensure upload directory exists
    if let Err(e) = fs::create_dir_all("uploads").await {
        tracing::warn!("Failed to create uploads directory: {}", e);
    }
    if let Err(e) = fs::create_dir_all("static").await {
        tracing::warn!("Failed to create static directory: {}", e);
    }

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health_check))
        .route("/debug/portfolios", get(debug_portfolios)) // Debug endpoint
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .route("/download/:filename", get(download_pdf))
        .route("/view/:id", get(view_pdf))
        .route("/like/:id", post(like_portfolio))
        // Protected admin routes with auth middleware
        .route("/admin", get(admin_page))
        .route("/admin/add", get(add_portfolio_page).post(add_portfolio))
        .route("/admin/delete/:id", post(delete_portfolio))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/uploads", ServeDir::new("uploads"))
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(DefaultBodyLimit::disable())
                .layer(RequestBodyLimitLayer::new(20 * 1024 * 1024)) // 20MB limit
                .layer(CompressionLayer::new())
                .layer(CorsLayer::permissive())
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("🚀 Server running on http://0.0.0.0:3000");
    
    axum::serve(listener, app).await?;
    Ok(())
}

async fn setup_database() -> Result<PgPool> {
    // Use environment variable or default to Docker PostgreSQL
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/portfolio_db".to_string());
    
    tracing::info!("Connecting to PostgreSQL: {}", db_url.replace("password", "***"));
    
    // Configure connection pool for concurrent access
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)      // Increase connection pool
        .min_connections(5)       // Keep minimum connections
        .acquire_timeout(Duration::from_secs(30))  // Timeout for getting connection
        .idle_timeout(Duration::from_secs(600))    // 10 minutes idle timeout
        .max_lifetime(Duration::from_secs(1800))   // 30 minutes max lifetime
        .connect(&db_url)
        .await?;
    
    // Create portfolios table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS portfolios (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            pdf_filename TEXT NOT NULL,
            likes BIGINT DEFAULT 0,
            views BIGINT DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
        "#,
    )
    .execute(&db)
    .await?;

    // Create index for performance
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_portfolios_created_at ON portfolios(created_at DESC)")
        .execute(&db)
        .await?;

    tracing::info!("✅ PostgreSQL database setup completed");
    Ok(db)
}

// Authentication middleware
async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<impl IntoResponse, StatusCode> {
    let path = request.uri().path();
    
    // Skip auth for non-admin routes
    if !path.starts_with("/admin") {
        return Ok(next.run(request).await);
    }
    
    // Check for valid session
    if let Some(cookie_header) = headers.get(COOKIE) {
        if let Ok(cookie_str) = cookie_header.to_str() {
            for cookie in cookie_str.split(';') {
                let cookie = cookie.trim();
                if cookie.starts_with("session=") {
                    let session_id = &cookie[8..]; // Remove "session=" prefix
                    
                    if let Some(session_time) = state.sessions.get(session_id) {
                        let now = Utc::now();
                        let elapsed = now.signed_duration_since(*session_time);
                        
                        // Session valid for 24 hours
                        if elapsed.num_hours() < 24 {
                            return Ok(next.run(request).await);
                        } else {
                            // Remove expired session
                            state.sessions.remove(session_id);
                        }
                    }
                }
            }
        }
    }
    
    // Redirect to login if not authenticated
    Ok(Redirect::to("/login").into_response())
}

async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("Health check called from IP");
    
    // Test database connection
    match sqlx::query("SELECT 1").fetch_one(&state.db).await {
        Ok(_) => {
            tracing::info!("Database connection OK");
            "OK - Database Connected"
        },
        Err(e) => {
            tracing::error!("Database connection failed: {}", e);
            "ERROR - Database Disconnected"
        }
    }
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("Index page requested");
    
    // Temporarily skip cache for debugging
    let portfolios = match sqlx::query_as::<_, Portfolio>(
        "SELECT id, title, description, pdf_filename, likes, views, created_at FROM portfolios ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await {
        Ok(portfolios) => {
            tracing::info!("Found {} portfolios", portfolios.len());
            portfolios
        },
        Err(e) => {
            tracing::error!("Database error in index: {}", e);
            vec![]
        }
    };

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
    
    // Skip cache for debugging
    tracing::info!("Returning HTML response ({} chars)", html.len());
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

async fn login(State(state): State<AppState>, Form(form): Form<LoginForm>) -> impl IntoResponse {
    // Simple hardcoded admin credentials (in production, use hashed passwords)
    if form.username == "admin" && form.password == "maliksigma" {
        // Create session
        let session_id = Uuid::new_v4().to_string();
        state.sessions.insert(session_id.clone(), Utc::now());
        
        // Set cookie and redirect
        let cookie = format!("session={}; Path=/; HttpOnly; Max-Age=86400", session_id); // 24 hours
        let mut response = Redirect::to("/admin").into_response();
        response.headers_mut().insert("Set-Cookie", cookie.parse().unwrap());
        
        response
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

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    // Remove session if exists
    if let Some(cookie_header) = headers.get(COOKIE) {
        if let Ok(cookie_str) = cookie_header.to_str() {
            for cookie in cookie_str.split(';') {
                let cookie = cookie.trim();
                if cookie.starts_with("session=") {
                    let session_id = &cookie[8..];
                    state.sessions.remove(session_id);
                }
            }
        }
    }
    
    // Clear cookie and redirect
    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert("Set-Cookie", "session=; Path=/; HttpOnly; Max-Age=0".parse().unwrap());
    response
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
        <form method="post" action="/admin/add" enctype="multipart/form-data" onsubmit="return validateForm()">
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
                <input type="file" id="pdf" name="pdf" accept=".pdf" required onchange="checkFileSize()">
                <small style="color: #ffff00; display: block; margin-top: 5px;">Upload your CTF writeup PDF (Max: 20MB)</small>
                <div id="fileError" style="color: #ff1020; display: none; margin-top: 5px;"></div>
            </div>
            <button type="submit">🚀 ADD WRITEUP</button>
        </form>
    </div>
    <script>
        function checkFileSize() {
            const fileInput = document.getElementById('pdf');
            const errorDiv = document.getElementById('fileError');
            const file = fileInput.files[0];
            
            if (file) {
                const maxSize = 20 * 1024 * 1024; // 20MB in bytes
                if (file.size > maxSize) {
                    errorDiv.textContent = `File too large! Size: ${(file.size / (1024 * 1024)).toFixed(2)}MB. Maximum allowed: 20MB`;
                    errorDiv.style.display = 'block';
                    fileInput.value = ''; // Clear the file input
                } else {
                    errorDiv.style.display = 'none';
                }
            }
        }
        
        function validateForm() {
            const fileInput = document.getElementById('pdf');
            const file = fileInput.files[0];
            
            if (file) {
                const maxSize = 20 * 1024 * 1024; // 20MB in bytes
                if (file.size > maxSize) {
                    alert('File too large! Maximum size is 20MB.');
                    return false;
                }
            }
            return true;
        }
    </script>
</body>
</html>
    "#;
    Html(html)
}

async fn add_portfolio(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut title = String::new();
    let mut description = String::new();
    let mut pdf_data: Option<Vec<u8>> = None;
    let mut pdf_filename = String::new();

    tracing::info!("Starting multipart processing");

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Error getting next field: {}", e);
        (StatusCode::BAD_REQUEST, format!("Multipart error: {}", e))
    })? {
        let name = field.name().unwrap_or("unknown").to_string();
        tracing::debug!("Processing field: {}", name);
        
        match name.as_str() {
            "title" => {
                title = field.text().await.map_err(|e| {
                    tracing::error!("Error reading title: {}", e);
                    (StatusCode::BAD_REQUEST, "Error reading title".to_string())
                })?;
                tracing::debug!("Title received: {}", title);
            }
            "description" => {
                description = field.text().await.map_err(|e| {
                    tracing::error!("Error reading description: {}", e);
                    (StatusCode::BAD_REQUEST, "Error reading description".to_string())
                })?;
                tracing::debug!("Description received ({} chars)", description.len());
            }
            "pdf" => {
                if let Some(filename) = field.file_name() {
                    let filename = filename.to_string(); // Clone filename before moving field
                    tracing::debug!("Processing PDF file: {}", filename);
                    
                    let data = field.bytes().await.map_err(|e| {
                        tracing::error!("Error reading PDF bytes: {}", e);
                        (StatusCode::BAD_REQUEST, "Error reading PDF file".to_string())
                    })?;
                    
                    tracing::info!("PDF data received ({} bytes)", data.len());
                    
                    // Check file size (20MB limit)
                    if data.len() > 20 * 1024 * 1024 {
                        tracing::warn!("File too large: {} bytes", data.len());
                        return Err((StatusCode::PAYLOAD_TOO_LARGE, "File too large! Maximum size is 20MB.".to_string()));
                    }
                    
                    let uuid = Uuid::new_v4().to_string();
                    pdf_filename = format!("{}_{}", uuid, filename);
                    pdf_data = Some(data.to_vec());
                } else {
                    // Consume field without filename
                    let _ = field.bytes().await;
                    tracing::warn!("PDF field with no filename");
                }
            }
            _ => {
                // Skip unknown fields
                let _ = field.bytes().await;
                tracing::debug!("Skipping unknown field: {}", name);
            }
        }
    }

    // Validate required fields
    if title.is_empty() || description.is_empty() || pdf_data.is_none() {
        tracing::warn!("Missing required fields - title: '{}', desc length: {}, has_pdf: {}", 
                     title, description.len(), pdf_data.is_some());
        return Err((StatusCode::BAD_REQUEST, "Missing required fields. Please fill all fields and select a PDF file.".to_string()));
    }

    // Write file to disk
    if let Some(data) = pdf_data {
        fs::write(format!("uploads/{}", pdf_filename), data).await.map_err(|e| {
            tracing::error!("File write error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error uploading file".to_string())
        })?;
        tracing::info!("File written successfully: {}", pdf_filename);
    }

    // Save to database
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO portfolios (id, title, description, pdf_filename) VALUES ($1, $2, $3, $4)"
    )
    .bind(&id)
    .bind(&title)
    .bind(&description)
    .bind(&pdf_filename)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Error saving to database".to_string())
    })?;

    tracing::info!("Portfolio added successfully with ID: {}", id);
    
    // Clear cache
    state.cache.remove("index");

    Ok(Redirect::to("/admin"))
}

async fn delete_portfolio(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Get filename first to delete file
    if let Ok(row) = sqlx::query("SELECT pdf_filename FROM portfolios WHERE id = $1")
        .bind(&id)
        .fetch_one(&state.db)
        .await
    {
        let filename: String = row.get("pdf_filename");
        let _ = fs::remove_file(format!("uploads/{}", filename)).await;
    }

    match sqlx::query("DELETE FROM portfolios WHERE id = $1")
        .bind(&id)
        .execute(&state.db)
        .await
    {
        Ok(_) => {
            println!("✅ DEBUG: Portfolio deleted successfully");
            // Clear cache
            state.cache.remove("index");
        }
        Err(e) => {
            println!("❌ DEBUG: Error deleting portfolio: {}", e);
            // Still clear cache and continue
            state.cache.remove("index");
        }
    }

    Redirect::to("/admin").into_response()
}

async fn download_pdf(Path(filename): Path<String>) -> impl IntoResponse {
    match fs::read(format!("uploads/{}", filename)).await {
        Ok(data) => {
            let mut headers = HeaderMap::new();
            if let Ok(content_type) = "application/pdf".parse() {
                headers.insert("Content-Type", content_type);
            }
            if let Ok(disposition) = format!("attachment; filename=\"{}\"", filename).parse() {
                headers.insert("Content-Disposition", disposition);
            }
            (StatusCode::OK, headers, data).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn view_pdf(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::info!("View PDF request for ID: {}", id);
    
    // Increment view count
    let _ = sqlx::query("UPDATE portfolios SET views = views + 1 WHERE id = $1")
        .bind(&id)
        .execute(&state.db)
        .await;

    // Clear cache since view count changed
    state.cache.remove("index");

    // Get portfolio info
    match sqlx::query("SELECT pdf_filename, title FROM portfolios WHERE id = $1")
        .bind(&id)
        .fetch_one(&state.db)
        .await
    {
        Ok(row) => {
            let filename: String = row.get("pdf_filename");
            let title: String = row.get("title");
            tracing::info!("Found portfolio '{}' with file: {}", title, filename);
            
            match fs::read(format!("uploads/{}", filename)).await {
                Ok(data) => {
                    tracing::info!("Successfully read PDF file: {} ({} bytes)", filename, data.len());
                    let mut headers = HeaderMap::new();
                    if let Ok(content_type) = "application/pdf".parse() {
                        headers.insert("Content-Type", content_type);
                    }
                    if let Ok(disposition) = format!("inline; filename=\"{}\"", filename).parse() {
                        headers.insert("Content-Disposition", disposition);
                    }
                    (StatusCode::OK, headers, data).into_response()
                }
                Err(e) => {
                    tracing::error!("Failed to read PDF file {}: {}", filename, e);
                    StatusCode::NOT_FOUND.into_response()
                }
            }
        }
        Err(e) => {
            tracing::warn!("Portfolio with ID {} not found: {}", id, e);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn like_portfolio(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Get client IP from headers (for proxy/load balancer setups) or use a default
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .or_else(|| headers.get("x-real-ip").and_then(|h| h.to_str().ok()))
        .unwrap_or("unknown")
        .to_string();
    
    let rate_key = format!("{}:{}", client_ip, id);
    
    // Check rate limiting (5 menit = 300 detik)
    if let Some(last_like_time) = state.rate_limiter.get(&rate_key) {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(*last_like_time);
        if elapsed.num_seconds() < 300 { // 5 menit
            tracing::debug!("Rate limit hit for IP {} on portfolio {}", client_ip, id);
            return (StatusCode::TOO_MANY_REQUESTS, "Rate limited: Wait 5 minutes before liking again").into_response();
        }
    }
    
    // Increment like count
    match sqlx::query("UPDATE portfolios SET likes = likes + 1 WHERE id = $1")
        .bind(&id)
        .execute(&state.db)
        .await
    {
        Ok(_) => {
            // Update rate limiter
            state.rate_limiter.insert(rate_key, Utc::now());
            
            // Clear cache since like count changed
            state.cache.remove("index");
            
            tracing::info!("Like added to portfolio {} from IP {}", id, client_ip);
            (StatusCode::OK, "Liked successfully").into_response()
        }
        Err(e) => {
            tracing::error!("Failed to add like to portfolio {}: {}", id, e);
            (StatusCode::NOT_FOUND, "Portfolio not found").into_response()
        }
    }
}

async fn debug_portfolios(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("Debug endpoint called - listing all portfolios");
    
    let portfolios = sqlx::query_as::<_, Portfolio>(
        "SELECT id, title, description, pdf_filename, likes, views, created_at FROM portfolios ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut response = format!("Total portfolios: {}\n\n", portfolios.len());
    
    for (idx, portfolio) in portfolios.iter().enumerate() {
        response.push_str(&format!(
            "{}. ID: {}\n   Title: {}\n   File: {}\n   Created: {}\n\n",
            idx + 1,
            portfolio.id,
            portfolio.title,
            portfolio.pdf_filename,
            portfolio.created_at
        ));
    }
    
    if portfolios.is_empty() {
        response.push_str("No portfolios found in database.\n");
    }
    
    response
}
