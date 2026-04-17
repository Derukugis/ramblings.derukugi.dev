use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use once_cell::sync::Lazy;
use pulldown_cmark::{html, Options, Parser};
use std::{
    path::{Path as StdPath},
    time::SystemTime,
};
use chrono::{DateTime, Utc};
use tera::{Context, Tera};
use tokio::fs::{self, read_dir};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

static TERA: Lazy<Tera> = Lazy::new(|| {
    Tera::new("templates/**/*.html").unwrap()
});

static MD_OPTIONS: Lazy<Options> = Lazy::new(|| {
    let mut opt = Options::empty();
    opt.insert(Options::ENABLE_TABLES);
    opt.insert(Options::ENABLE_STRIKETHROUGH);
    opt.insert(Options::ENABLE_FOOTNOTES);
    opt.insert(Options::ENABLE_TASKLISTS);
    opt.insert(Options::ENABLE_SUPERSCRIPT);
    opt
});

static INDEX_HTML: Lazy<String> = Lazy::new(|| {
    TERA.render("index.html", &Context::new()).unwrap()
});

async fn index() -> Html<String> {
    Html(INDEX_HTML.clone())
}

async fn ramblings() -> Html<String> {
    let mut context = Context::new();
    context.insert("title", "ramblings.derukugi.dev");

    let articles_raw = collect_articles("./templates/md").await;
    let mut articles: Vec<(String, String)> = articles_raw
        .into_iter()
        .map(|(name, time)| {
            let datetime: DateTime<Utc> = time.into();
            (name, datetime.format("%Y-%m-%d").to_string())
        })
        .collect();
    articles.sort_by(|a, b| b.1.cmp(&a.1));

    context.insert("articles", &articles);

    Html(TERA.render("ramblings.html", &context).unwrap())
}

async fn article(Path(wildcard): Path<String>) -> impl IntoResponse {
    let file_path = format!("./templates/md/{}.md", wildcard);

    let markdown_input = match fs::read_to_string(&file_path).await {
        Ok(s) => s,
        Err(_) => return (StatusCode::NOT_FOUND, "Article not found").into_response(),
    };

    let parser = Parser::new_ext(&markdown_input, *MD_OPTIONS);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    let date = match fs::metadata(&file_path).await {
        Ok(meta) => match meta.modified() {
            Ok(time) => {
                let datetime: DateTime<Utc> = time.into();
                datetime.format("%Y-%m-%d").to_string()
            }
            Err(_) => String::new(),
        },
        Err(_) => String::new(),
    };

    let mut context = Context::new();
    context.insert("content", &html_output);
    context.insert("title", markdown_input.lines().next().unwrap_or(""));
    context.insert("name", &wildcard);
    context.insert("date", &date);

    Html(TERA.render("article.html", &context).unwrap()).into_response()
}

async fn collect_articles(path: impl AsRef<StdPath>) -> Vec<(String, SystemTime)> {
    let mut result = Vec::new();
    let mut stack = vec![path.as_ref().to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = match read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();

            match entry.metadata().await {
                Ok(meta) if meta.is_dir() => stack.push(path),
                Ok(meta) if meta.is_file() => {
                    if path.extension().and_then(|e| e.to_str()) != Some("md") {
                        continue;
                    }
                    if let (Some(name), Ok(modified)) = (
                        path.file_stem().and_then(|s| s.to_str()),
                        meta.modified(),
                    ) {
                        result.push((name.to_string(), modified));
                    }
                }
                _ => {}
            }
        }
    }

    result
}

#[tokio::main]
async fn main() {
    let static_files_service = ServeDir::new("templates/public");

    let app = Router::new()
        .route("/", get(index))
        .route("/ramblings", get(ramblings))
        .route("/ramblings/{*wildcard}", get(article))
        .nest_service("/public", static_files_service);

    let listener = TcpListener::bind("0.0.0.0:4321").await.unwrap();
    println!("http://0.0.0.0:4321");
    axum::serve(listener, app).await.unwrap();
}
