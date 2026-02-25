use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::collections::BTreeMap;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenvy::dotenv().expect("Failed to load .env file");

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env");
    let schema = std::env::var("DB_SCHEMA").unwrap_or_else(|_| "public".to_string());

    let args: Vec<String> = std::env::args().collect();
    let table_arg = args.get(1).cloned();

    println!("Connecting to database...");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    match table_arg {
        Some(name) => {
            // Detect whether name is a table, view, or materialized view
            let kind: Option<String> = sqlx::query_scalar(
                r#"
                SELECT relkind::text
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = $1 AND c.relname = $2
                  AND c.relkind IN ('r', 'v', 'm')
                "#,
            )
            .bind(&schema)
            .bind(&name)
            .fetch_optional(&pool)
            .await?;

            match kind.as_deref() {
                Some("r") => {
                    println!("Generating CREATE script for '{schema}.{name}'...\n");
                    generate_create_script(&pool, &schema, &name).await?;
                }
                Some("v") | Some("m") => {
                    let label = if kind.as_deref() == Some("m") {
                        "materialized view"
                    } else {
                        "view"
                    };
                    println!("Dependencies for {label} '{schema}.{name}':\n");
                    view_dependencies(&pool, &schema, &name).await?;
                }
                _ => {
                    println!("No table or view named '{name}' found in schema '{schema}'.");
                }
            }
        }
        None => {
            println!("Connected! Fetching objects in schema '{schema}'...\n");
            list_tables(&pool, &schema).await?;
            println!();
            list_views(&pool, &schema).await?;
        }
    }

    Ok(())
}

async fn list_tables(pool: &sqlx::PgPool, schema: &str) -> Result<(), sqlx::Error> {
    let tables: Vec<String> = sqlx::query(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = $1
          AND table_type = 'BASE TABLE'
        ORDER BY table_name
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
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

async fn list_views(pool: &sqlx::PgPool, schema: &str) -> Result<(), sqlx::Error> {
    let views: Vec<(String, String)> = sqlx::query(
        r#"
        SELECT relname AS view_name,
               CASE relkind WHEN 'v' THEN 'VIEW' ELSE 'MATERIALIZED VIEW' END AS kind
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1 AND c.relkind IN ('v', 'm')
        ORDER BY relkind, relname
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("view_name"), row.get("kind")))
    .collect();

    if views.is_empty() {
        println!("No views found in schema '{schema}'.");
    } else {
        println!("{:<5} {:<20} {}", "#", "Kind", "View Name");
        println!("{}", "-".repeat(50));
        for (i, (name, kind)) in views.iter().enumerate() {
            println!("{:<5} {:<20} {}", i + 1, kind, name);
        }
        println!("\nTotal: {} view(s)", views.len());
    }

    Ok(())
}

async fn view_dependencies(
    pool: &sqlx::PgPool,
    schema: &str,
    view: &str,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(String, String, String)> = sqlx::query(
        r#"
        SELECT DISTINCT
            dep_ns.nspname   AS dep_schema,
            dep_obj.relname  AS dep_name,
            CASE dep_obj.relkind
                WHEN 'r' THEN 'TABLE'
                WHEN 'v' THEN 'VIEW'
                WHEN 'm' THEN 'MATERIALIZED VIEW'
                ELSE dep_obj.relkind::text
            END AS dep_kind
        FROM pg_rewrite rw
        JOIN pg_depend d       ON d.objid     = rw.oid
        JOIN pg_class dep_obj  ON dep_obj.oid = d.refobjid
        JOIN pg_namespace dep_ns ON dep_ns.oid = dep_obj.relnamespace
        JOIN pg_class view_cls ON view_cls.oid = rw.ev_class
        JOIN pg_namespace view_ns ON view_ns.oid = view_cls.relnamespace
        WHERE view_cls.relname = $1
          AND view_ns.nspname  = $2
          AND dep_obj.relname != $1
          AND dep_obj.relkind IN ('r', 'v', 'm')
        ORDER BY dep_kind, dep_schema, dep_name
        "#,
    )
    .bind(view)
    .bind(schema)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("dep_schema"), row.get("dep_name"), row.get("dep_kind")))
    .collect();

    if rows.is_empty() {
        println!("No dependencies found (view may reference only literals or functions).");
    } else {
        println!("{:<5} {:<20} {}", "#", "Kind", "Object");
        println!("{}", "-".repeat(55));
        for (i, (dep_schema, dep_name, dep_kind)) in rows.iter().enumerate() {
            let qualified = if dep_schema == schema {
                dep_name.clone()
            } else {
                format!("{dep_schema}.{dep_name}")
            };
            println!("{:<5} {:<20} {}", i + 1, dep_kind, qualified);
        }
    }

    // Also show the view definition
    let def: Option<String> = sqlx::query_scalar(
        r#"
        SELECT pg_get_viewdef(c.oid, true)
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1 AND c.relname = $2
          AND c.relkind IN ('v', 'm')
        "#,
    )
    .bind(schema)
    .bind(view)
    .fetch_optional(pool)
    .await?
    .flatten();

    if let Some(definition) = def {
        println!("\n-- View definition --");
        println!("{definition}");
    }

    Ok(())
}

async fn generate_create_script(
    pool: &sqlx::PgPool,
    schema: &str,
    table: &str,
) -> Result<(), sqlx::Error> {
    // ── columns ──────────────────────────────────────────────────────────────
    let col_rows = sqlx::query(
        r#"
        SELECT column_name, data_type, udt_name,
               character_maximum_length,
               numeric_precision, numeric_scale,
               is_nullable, column_default
        FROM information_schema.columns
        WHERE table_schema = $1 AND table_name = $2
        ORDER BY ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?;

    if col_rows.is_empty() {
        println!("Table '{schema}.{table}' not found or has no columns.");
        return Ok(());
    }

    // ── primary key ───────────────────────────────────────────────────────────
    let pk_cols: Vec<String> = sqlx::query(
        r#"
        SELECT kcu.column_name
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema    = kcu.table_schema
        WHERE tc.constraint_type = 'PRIMARY KEY'
          AND tc.table_schema = $1 AND tc.table_name = $2
        ORDER BY kcu.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.get::<String, _>("column_name"))
    .collect();

    // ── unique constraints ────────────────────────────────────────────────────
    let mut unique_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in sqlx::query(
        r#"
        SELECT tc.constraint_name, kcu.column_name
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema    = kcu.table_schema
        WHERE tc.constraint_type = 'UNIQUE'
          AND tc.table_schema = $1 AND tc.table_name = $2
        ORDER BY tc.constraint_name, kcu.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    {
        unique_map
            .entry(row.get("constraint_name"))
            .or_default()
            .push(row.get("column_name"));
    }

    // ── foreign keys ──────────────────────────────────────────────────────────
    struct FkInfo {
        columns: Vec<String>,
        foreign_schema: String,
        foreign_table: String,
        foreign_columns: Vec<String>,
        update_rule: String,
        delete_rule: String,
    }
    let mut fk_map: BTreeMap<String, FkInfo> = BTreeMap::new();
    for row in sqlx::query(
        r#"
        SELECT tc.constraint_name,
               kcu.column_name,
               ccu.table_schema  AS foreign_schema,
               ccu.table_name    AS foreign_table,
               ccu.column_name   AS foreign_column,
               rc.update_rule,
               rc.delete_rule
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema    = kcu.table_schema
        JOIN information_schema.constraint_column_usage ccu
          ON ccu.constraint_name  = tc.constraint_name
         AND ccu.constraint_schema = tc.table_schema
        JOIN information_schema.referential_constraints rc
          ON rc.constraint_name  = tc.constraint_name
         AND rc.constraint_schema = tc.table_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
          AND tc.table_schema = $1 AND tc.table_name = $2
        ORDER BY tc.constraint_name, kcu.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    {
        let cname: String = row.get("constraint_name");
        let entry = fk_map.entry(cname).or_insert_with(|| FkInfo {
            columns: vec![],
            foreign_schema: row.get("foreign_schema"),
            foreign_table: row.get("foreign_table"),
            foreign_columns: vec![],
            update_rule: row.get("update_rule"),
            delete_rule: row.get("delete_rule"),
        });
        entry.columns.push(row.get("column_name"));
        entry.foreign_columns.push(row.get("foreign_column"));
    }

    // ── check constraints ─────────────────────────────────────────────────────
    let check_rows = sqlx::query(
        r#"
        SELECT tc.constraint_name, cc.check_clause
        FROM information_schema.table_constraints tc
        JOIN information_schema.check_constraints cc
          ON cc.constraint_name  = tc.constraint_name
         AND cc.constraint_schema = tc.table_schema
        WHERE tc.constraint_type = 'CHECK'
          AND tc.table_schema = $1 AND tc.table_name = $2
          AND tc.constraint_name NOT LIKE '%_not_null'
        ORDER BY tc.constraint_name
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?;

    // ── assemble script ───────────────────────────────────────────────────────
    let mut parts: Vec<String> = vec![];

    for row in &col_rows {
        let col_name: String = row.get("column_name");
        let data_type: String = row.get("data_type");
        let udt_name: String = row.get("udt_name");
        let char_max: Option<i32> = row.get("character_maximum_length");
        let num_prec: Option<i32> = row.get("numeric_precision");
        let num_scale: Option<i32> = row.get("numeric_scale");
        let is_nullable: String = row.get("is_nullable");
        let default: Option<String> = row.get("column_default");

        let type_str = pg_type(&data_type, &udt_name, char_max, num_prec, num_scale);
        let mut def = format!("    {col_name} {type_str}");
        if let Some(d) = default {
            def.push_str(&format!(" DEFAULT {d}"));
        }
        if is_nullable == "NO" && !pk_cols.contains(&col_name) {
            def.push_str(" NOT NULL");
        }
        parts.push(def);
    }

    if !pk_cols.is_empty() {
        parts.push(format!("    PRIMARY KEY ({})", pk_cols.join(", ")));
    }
    for (cname, cols) in &unique_map {
        parts.push(format!(
            "    CONSTRAINT {cname} UNIQUE ({})",
            cols.join(", ")
        ));
    }
    for (cname, fk) in &fk_map {
        let ref_table = if fk.foreign_schema == schema {
            fk.foreign_table.clone()
        } else {
            format!("{}.{}", fk.foreign_schema, fk.foreign_table)
        };
        let mut s = format!(
            "    CONSTRAINT {cname} FOREIGN KEY ({}) REFERENCES {ref_table} ({})",
            fk.columns.join(", "),
            fk.foreign_columns.join(", ")
        );
        if fk.update_rule != "NO ACTION" {
            s.push_str(&format!(" ON UPDATE {}", fk.update_rule));
        }
        if fk.delete_rule != "NO ACTION" {
            s.push_str(&format!(" ON DELETE {}", fk.delete_rule));
        }
        parts.push(s);
    }
    for row in &check_rows {
        let cname: String = row.get("constraint_name");
        let clause: String = row.get("check_clause");
        parts.push(format!("    CONSTRAINT {cname} CHECK {clause}"));
    }

    println!("CREATE TABLE {schema}.{table} (");
    println!("{}", parts.join(",\n"));
    println!(");");

    Ok(())
}

fn pg_type(
    data_type: &str,
    udt_name: &str,
    char_max: Option<i32>,
    num_prec: Option<i32>,
    num_scale: Option<i32>,
) -> String {
    match data_type {
        "character varying" => char_max
            .map(|n| format!("VARCHAR({n})"))
            .unwrap_or_else(|| "VARCHAR".into()),
        "character" => char_max
            .map(|n| format!("CHAR({n})"))
            .unwrap_or_else(|| "CHAR".into()),
        "numeric" | "decimal" => match (num_prec, num_scale) {
            (Some(p), Some(s)) if s > 0 => format!("NUMERIC({p}, {s})"),
            (Some(p), _) => format!("NUMERIC({p})"),
            _ => "NUMERIC".into(),
        },
        "integer" => "INTEGER".into(),
        "bigint" => "BIGINT".into(),
        "smallint" => "SMALLINT".into(),
        "real" => "REAL".into(),
        "double precision" => "DOUBLE PRECISION".into(),
        "boolean" => "BOOLEAN".into(),
        "text" => "TEXT".into(),
        "bytea" => "BYTEA".into(),
        "date" => "DATE".into(),
        "time without time zone" => "TIME".into(),
        "time with time zone" => "TIMETZ".into(),
        "timestamp without time zone" => "TIMESTAMP".into(),
        "timestamp with time zone" => "TIMESTAMPTZ".into(),
        "interval" => "INTERVAL".into(),
        "uuid" => "UUID".into(),
        "json" => "JSON".into(),
        "jsonb" => "JSONB".into(),
        "ARRAY" => format!("{}[]", udt_name.trim_start_matches('_')),
        "USER-DEFINED" => udt_name.into(),
        _ => data_type.to_uppercase(),
    }
}
