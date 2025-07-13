use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use askama::Template;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};
use std::{collections::HashMap, sync::Arc};
use tower_http::{
    compression::CompressionLayer,
    services::ServeDir,
    cors::CorsLayer,
};
use dashmap::DashMap;
use tokio::fs;
use uuid::Uuid;
use chrono::{DateTime, Utc};

// Cache untuk performa
type Cache = Arc<DashMap<String, CacheEntry>>;

#[derive(Clone)]
struct CacheEntry {
    data: String,
    timestamp: DateTime<Utc>,
}

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    cache: Cache,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    portfolios: Vec<Portfolio>,
}

#[derive(Template)]
#[template(path = "admin.html")]
struct AdminTemplate {
    portfolios: Vec<Portfolio>,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "add_portfolio.html")]
struct AddPortfolioTemplate;

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
    tracing_subscriber::init();

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
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("🚀 Server running on http://0.0.0.0:3000");
    
    axum::serve(listener, app).await?;
    Ok(())
}

async fn setup_database() -> anyhow::Result<SqlitePool> {
    let db = SqlitePool::connect("sqlite:database.db").await?;
    
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

    Ok(db)
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    // Check cache first
    if let Some(cached) = state.cache.get("index") {
        if Utc::now().signed_duration_since(cached.timestamp).num_seconds() < 300 {
            return Html(cached.data.clone());
        }
    }

    let portfolios = sqlx::query_as::<_, Portfolio>(
        "SELECT id, title, description, pdf_filename, created_at FROM portfolios ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let template = IndexTemplate { portfolios };
    let html = template.render().unwrap();
    
    // Cache the result
    state.cache.insert("index".to_string(), CacheEntry {
        data: html.clone(),
        timestamp: Utc::now(),
    });

    Html(html)
}

async fn login_page() -> impl IntoResponse {
    let template = LoginTemplate { error: None };
    Html(template.render().unwrap())
}

async fn login(Form(form): Form<LoginForm>) -> impl IntoResponse {
    // Simple hardcoded admin credentials (in production, use hashed passwords)
    if form.username == "admin" && form.password == "admin123" {
        Redirect::to("/admin")
    } else {
        let template = LoginTemplate { 
            error: Some("Invalid credentials".to_string()) 
        };
        Html(template.render().unwrap()).into_response()
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

    let template = AdminTemplate { portfolios };
    Html(template.render().unwrap())
}

async fn add_portfolio_page() -> impl IntoResponse {
    let template = AddPortfolioTemplate;
    Html(template.render().unwrap())
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
