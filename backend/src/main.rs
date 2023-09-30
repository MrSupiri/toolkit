#[deny(clippy::all)]
use std::env;
use dotenv::dotenv;
use poem::EndpointExt;
use poem::{listener::TcpListener, Route, Server};
use poem_openapi::OpenApiService;
use sqlx::{sqlite::SqliteConnectOptions, Error, SqlitePool};
use std::{future::Future, path::Path};

mod fcm;


async fn connect(filename: impl AsRef<Path>) -> impl Future<Output = Result<SqlitePool, Error>> {
    let filename = filename.as_ref().to_str().unwrap().trim_start_matches("sqlite:");

    let options = SqliteConnectOptions::new()
        .filename(filename)
        .create_if_missing(true);

    SqlitePool::connect_with(options)
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    dotenv().ok(); // This line loads the environment variables from the ".env" file.
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    tracing_subscriber::fmt::init();

    let pool = connect(database_url).await.await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();

    let api_service = OpenApiService::new(fcm::FCMAPI::default(), "ToolKit", "1.0")
        .server("http://0.0.0.0:3000/api");
    let ui = api_service.swagger_ui();

    let route = Route::new()
        .nest("/api", api_service)
        .nest("/", ui)
        .data(pool);

    Server::new(TcpListener::bind("0.0.0.0:3000"))
        .run(route)
        .await
}
