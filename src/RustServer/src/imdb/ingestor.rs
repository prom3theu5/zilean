use crate::imdb::ImdbSearcher;
use crate::utils;
use arc_swap::ArcSwap;
use flate2::read::GzDecoder;
use reqwest::Client;
use serde::Serialize;
use sqlx::postgres::{PgConnectOptions, PgConnection};
use sqlx::types::Json;
use sqlx::{ConnectOptions, Connection};
use std::collections::HashSet;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
    sync::Arc,
    time::Duration,
};
use tantivy::TantivyDocument;
use tokio::{fs, io::AsyncWriteExt};
use tracing::log::LevelFilter;

pub struct ImdbIngestor {
    searcher: Arc<ArcSwap<ImdbSearcher>>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
enum ImportStatus {
    InProgress,
    Complete,
    Failed,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct ImdbLastImport {
    OccuredAt: chrono::DateTime<chrono::Utc>,
    EntryCount: i64,
    Status: ImportStatus,
}

const REQUIRED_CATEGORIES: &[&str] = &[
    "movie",
    "tvMovie",
    "tvSeries",
    "tvShort",
    "tvMiniSeries",
    "tvSpecial",
];

impl ImdbIngestor {
    pub fn new(searcher: Arc<ArcSwap<ImdbSearcher>>) -> Self {
        Self { searcher }
    }

    pub async fn ingest_imdb_data(
        &self,
        force_download: bool,
        force_index: bool,
    ) -> anyhow::Result<usize> {
        let db_url = std::env::var("ZILEAN_DATABASE_URL").expect("ZILEAN_DATABASE_URL must be set");
        let file_name = "title.basics.tsv";
        let base_url = "https://datasets.imdbws.com/";
        let data_dir = Path::new("./data");
        let data_file = data_dir.join(file_name);
        let temp_gz_path = data_dir.join(format!("{file_name}.gz"));

        fs::create_dir_all(data_dir).await?;

        let mut valid_cached_file = false;

        if !force_download && data_file.exists() {
            let metadata = fs::metadata(&data_file).await?;
            if let Ok(modified) = metadata.modified() {
                if modified.elapsed().unwrap_or(Duration::MAX) < Duration::from_secs(30 * 86400) {
                    tracing::info!("Using cached IMDb file at {}", data_file.display());
                    valid_cached_file = true;
                } else {
                    tracing::info!("Cached IMDb file is older than 30 days, re-downloading...");
                    fs::remove_file(&data_file).await.ok();
                }
            }
        } else {
            fs::remove_file(&data_file).await.ok();
        }

        if !data_file.exists() {
            let client = Client::new();
            let mut resp = client
                .get(format!("{base_url}{file_name}.gz"))
                .header("User-Agent", "curl/7.54")
                .send()
                .await?
                .error_for_status()?;

            let mut gz_file = fs::File::create(&temp_gz_path).await?;
            while let Some(chunk) = resp.chunk().await? {
                gz_file.write_all(&chunk).await?;
            }
            gz_file.flush().await?;

            let mut gz = GzDecoder::new(File::open(&temp_gz_path)?);
            let mut out_file = File::create(&data_file)?;
            std::io::copy(&mut gz, &mut out_file)?;
            fs::remove_file(&temp_gz_path).await.ok();
        }

        self.load_and_index(&data_file, &db_url, force_index, valid_cached_file)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_and_index(
        &self,
        tsv_path: &Path,
        db_url: &String,
        force_index: bool,
        valid_cached_file: bool,
    ) -> anyhow::Result<usize> {
        let current = self.searcher.load();
        let mut new_searcher = (*current).clone();
        Arc::make_mut(&mut new_searcher).drop_and_initialise_index()?;
        tracing::info!("Re-initialised Tantivy index");

        if valid_cached_file && !force_index {
            tracing::info!("Valid cached file, and force indexing is false, using existing index.");
            return Ok(0);
        }

        let connect_options = db_url
            .parse::<PgConnectOptions>()?
            .log_statements(LevelFilter::Debug)
            .log_slow_statements(LevelFilter::Info, Duration::from_secs(120));

        let mut db_conn = PgConnection::connect_with(&connect_options).await?;

        Self::drop_temp_table(&mut db_conn).await?;
        Self::create_staging_table(&mut db_conn).await?;

        let file = File::open(tsv_path)?;
        let reader = BufReader::new(file);
        let mut count = 0;

        let copy_stmt = r#"COPY imdbfiles_staging ("ImdbId", "Adult", "Category", "Title", "Year") FROM STDIN WITH (FORMAT text)"#;
        let mut copy_in = db_conn.copy_in_raw(copy_stmt).await?;

        let required_categories: HashSet<&str> = REQUIRED_CATEGORIES.iter().cloned().collect();
        let mut index_writer = new_searcher.index.writer(300_000_000)?;

        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            if i == 0 || line.starts_with("tconst") {
                continue;
            }

            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 9 {
                continue;
            }

            let imdb_id = parts[0];
            let category = parts[1];
            if !required_categories.contains(category) {
                continue;
            }
            let title = parts[2].replace('\t', " ").replace('\n', " ");
            let is_adult = parts[4] == "1";
            let year = parts[5].parse::<i32>().unwrap_or(0);

            // Tantivy doc
            let mut doc = TantivyDocument::default();
            doc.add_text(new_searcher.imdb_id, imdb_id);
            doc.add_text(new_searcher.category, category);
            doc.add_text(new_searcher.title, &title);
            doc.add_i64(new_searcher.year, year as i64);
            doc.add_text(
                new_searcher.normalized_title,
                utils::strings::normalize_title(&title),
            );
            index_writer.add_document(doc)?;

            // COPY line
            let copy_line = format!("{imdb_id}\t{is_adult}\t{category}\t{title}\t{year}\n");
            copy_in.send(copy_line.as_bytes()).await?;

            count += 1;
            if count % 100_000 == 0 {
                tracing::info!("Processed {} rows...", count);
            }
        }

        // Finalize
        tracing::info!("Committing Index...");
        index_writer.commit()?;
        self.searcher.store(new_searcher.clone());

        tracing::info!("Committing Postgres COPY ...");
        copy_in.finish().await?;

        Self::merge_temp_table_to_main(&mut db_conn).await?;
        Self::drop_temp_table(&mut db_conn).await?;
        Self::set_imdb_last_import(&mut db_conn, count as i64).await?;
        Self::vacuum_imdb_index(&mut db_conn).await?;
        Ok(count)
    }

    async fn set_imdb_last_import(
        db_conn: &mut PgConnection,
        entry_count: i64,
    ) -> anyhow::Result<()> {
        let last_import = ImdbLastImport {
            OccuredAt: chrono::Utc::now(),
            EntryCount: entry_count,
            Status: ImportStatus::Complete,
        };
        let value = serde_json::to_value(&last_import)?;

        sqlx::query(
            r#"
                INSERT INTO "ImportMetadata" ("Key", "Value")
                VALUES ($1, $2)
                ON CONFLICT ("Key") DO UPDATE SET "Value" = EXCLUDED."Value"
                "#,
        )
        .bind("ImdbLastImport")
        .bind(Json(value))
        .execute(db_conn)
        .await?;

        tracing::info!(
            "Indexing completed at {}, inserted {} IMDb records",
            last_import.OccuredAt,
            entry_count
        );
        Ok(())
    }

    async fn vacuum_imdb_index(db_conn: &mut PgConnection) -> anyhow::Result<()> {
        tracing::info!("Vacuuming IMDb index...");
        sqlx::query(r#"VACUUM (VERBOSE, ANALYZE) "ImdbFiles""#)
            .execute(db_conn)
            .await?;
        Ok(())
    }

    async fn drop_temp_table(db_conn: &mut PgConnection) -> anyhow::Result<()> {
        sqlx::query(r#"DROP TABLE IF EXISTS imdbfiles_staging"#)
            .execute(db_conn)
            .await?;
        Ok(())
    }

    async fn merge_temp_table_to_main(db_conn: &mut PgConnection) -> anyhow::Result<()> {
        tracing::info!("Postgres COPY completed, Merging onto main table...");
        sqlx::query(
            r#"
            INSERT INTO "ImdbFiles" ("ImdbId", "Adult", "Category", "Title", "Year")
            SELECT "ImdbId", "Adult", "Category", "Title", "Year" FROM imdbfiles_staging
            ON CONFLICT ("ImdbId") DO UPDATE
            SET "Adult"=EXCLUDED."Adult",
                "Category"=EXCLUDED."Category",
                "Title"=EXCLUDED."Title",
                "Year"=EXCLUDED."Year";
        "#,
        )
        .execute(db_conn)
        .await?;
        Ok(())
    }

    async fn create_staging_table(db_conn: &mut PgConnection) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TEMP TABLE imdbfiles_staging (
                "ImdbId" text PRIMARY KEY,
                "Adult" boolean,
                "Category" text,
                "Title" text,
                "Year" integer
            );
        "#,
        )
        .execute(db_conn)
        .await?;
        Ok(())
    }
}
