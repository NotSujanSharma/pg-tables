use sqlx::postgres::PgPoolOptions;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    // Load .env file
    dotenvy::dotenv().expect("Failed to load .env file");

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env");
    let schema = std::env::var("DB_SCHEMA").unwrap_or_else(|_| "public".to_string());

    println!("Connecting to database...");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("Connected! Fetching tables in schema '{schema}'...\n");

    let tables: Vec<String> = sqlx::query(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = $1
          AND table_type = 'BASE TABLE'
        ORDER BY table_name
        "#,
    )
    .bind(&schema)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|row| row.get::<String, _>("table_name"))
    .collect();

    if tables.is_empty() {
        println!("No tables found in schema '{schema}'.");
    } else {
        println!("{:<5} {}", "#", "Table Name");
        println!("{}", "-".repeat(40));
        for (i, name) in tables.iter().enumerate() {
            println!("{:<5} {}", i + 1, name);
        }
        println!("\nTotal: {} table(s)", tables.len());
    }

    Ok(())
}
