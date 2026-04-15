use std::collections::HashSet;
use std::fs;
use std::ops::Bound;
use std::path::Path;
use strsim::levenshtein;
use tantivy::query::{FuzzyTermQuery, Occur, PhraseQuery, Query, RangeQuery, TermQuery};
use tantivy::schema::{Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, TEXT, Value};
use tantivy::{Index, IndexReader, ReloadPolicy, TantivyDocument, Term};
use tracing::{debug, info};

pub const INDEX_PATH: &str = "./data/tantivy_index";

/// A single IMDb-index hit. Previously came from the proto module; now a
/// plain domain struct the searcher owns.
#[derive(Clone, Debug)]
pub struct Match {
    pub imdb_id: String,
    pub title: String,
    pub year: i32,
    pub score: f32,
}

#[derive(Clone)]
pub struct ImdbSearcher {
    pub index: Index,
    pub reader: IndexReader,
    pub minimum_score: f32,
    pub title: Field,
    pub category: Field,
    pub year: Field,
    pub imdb_id: Field,
    pub normalized_title: Field,
}

fn get_text(doc: &TantivyDocument, field: Field) -> Option<String> {
    match doc.get_first(field)?.as_value().as_str() {
        Some(s) => Some(s.to_string()),
        None => None,
    }
}

fn get_i64(doc: &TantivyDocument, field: Field) -> Option<i32> {
    match doc.get_first(field)?.as_value().as_i64() {
        Some(v) => Some(v as i32),
        None => None,
    }
}

pub fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("title", TEXT | STORED);
    schema_builder.add_text_field("category", STRING | STORED);
    schema_builder.add_i64_field("year", INDEXED | STORED);
    schema_builder.add_text_field("imdb_id", STRING | STORED);
    schema_builder.add_text_field("normalized_title", TEXT | STORED);
    schema_builder.build()
}

impl ImdbSearcher {
    pub fn new(minimum_score: f32) -> tantivy::Result<Self> {
        let index_path = Path::new(INDEX_PATH);
        if !index_path.exists() {
            fs::create_dir_all(index_path)?;
            info!("Creating new Tantivy index at {}", index_path.display());

            let schema = build_schema();
            let _ = Index::create_in_dir(index_path, schema)?;
            info!("Initialized empty Tantivy index");
        }

        let index = Index::open_in_dir(index_path)?;
        let schema = index.schema(); // <-- load the stored schema, not a new one

        let imdb_id = schema.get_field("imdb_id")?;
        let title = schema.get_field("title")?;
        let category = schema.get_field("category")?;
        let year = schema.get_field("year")?;
        let normalized_title = schema.get_field("normalized_title")?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            title,
            category,
            year,
            imdb_id,
            normalized_title,
            minimum_score,
        })
    }

    pub fn drop_and_initialise_index(&mut self) -> tantivy::Result<()> {
        info!("Dropping and re-initialising Tantivy index");
        let index_path = Path::new(INDEX_PATH);
        if index_path.exists() {
            fs::remove_dir_all(index_path)?;
        }
        fs::create_dir_all(index_path)?;
        let schema = build_schema();
        self.index = Index::create_in_dir(index_path, schema)?;

        self.reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        self.imdb_id = self.index.schema().get_field("imdb_id")?;
        self.title = self.index.schema().get_field("title")?;
        self.category = self.index.schema().get_field("category")?;
        self.year = self.index.schema().get_field("year")?;
        self.normalized_title = self.index.schema().get_field("normalized_title")?;
        Ok(())
    }

    pub fn search(&self, title: &str, category: &str, year: i32) -> Vec<Match> {
        let searcher = self.reader.searcher();

        let mut clauses = vec![];

        // Match exact title
        clauses.push((
            Occur::Should,
            Box::new(TermQuery::new(
                Term::from_field_text(self.normalized_title, title),
                IndexRecordOption::Basic,
            )) as Box<dyn Query>,
        ));

        // Boosted phrase query
        let phrase_terms: Vec<_> = title
            .split_whitespace()
            .map(|word| Term::from_field_text(self.normalized_title, word))
            .collect();

        if phrase_terms.len() > 1 {
            clauses.push((
                Occur::Should,
                Box::new(tantivy::query::BoostQuery::new(
                    Box::new(PhraseQuery::new(phrase_terms)),
                    2.0,
                )) as Box<dyn Query>,
            ));
        }

        // Fuzzy match per word in title
        for word in title.split_whitespace() {
            clauses.push((
                Occur::Should,
                Box::new(FuzzyTermQuery::new(
                    Term::from_field_text(self.normalized_title, word),
                    1,
                    true,
                )) as Box<dyn Query>,
            ));
        }

        // Category must match
        clauses.push((
            Occur::Must,
            Box::new(TermQuery::new(
                Term::from_field_text(self.category, category),
                IndexRecordOption::Basic,
            )) as Box<dyn Query>,
        ));

        // Year SHOULD match +/- 1 year
        if year > 0 {
            clauses.push((
                Occur::Should,
                Box::new(RangeQuery::new(
                    Bound::Included(Term::from_field_i64(self.year, year as i64 - 1)),
                    Bound::Included(Term::from_field_i64(self.year, year as i64 + 1)),
                )) as Box<dyn Query>,
            ));
        }

        let query = tantivy::query::BooleanQuery::new(clauses);

        let top_docs = searcher
            .search(&query, &tantivy::collector::TopDocs::with_limit(5))
            .unwrap_or_default();

        let mut results: Vec<Match> = top_docs
            .into_iter()
            .filter_map(|(score, addr)| {
                let retrieved: TantivyDocument = searcher.doc(addr).ok()?;
                Some(Match {
                    imdb_id: get_text(&retrieved, self.imdb_id)?,
                    title: get_text(&retrieved, self.normalized_title)?,
                    year: get_i64(&retrieved, self.year).unwrap_or(0),
                    score,
                })
            })
            .collect();

        self.apply_filters(title, &mut results)
    }

    fn apply_filters(&self, title: &str, results: &mut Vec<Match>) -> Vec<Match> {
        if results.is_empty() {
            debug!("No matches found for title: {}", title);
            return Vec::new();
        }

        let top_score = results.first().map(|m| m.score).unwrap_or(1.0);
        let min_absolute_score = top_score * self.minimum_score;
        results.retain(|r| r.score >= min_absolute_score);

        if results.is_empty() {
            debug!("No matches passed minimum score threshold for '{}'.", title);
            return Vec::new();
        }

        let query_tokens: Vec<_> = title.split_whitespace().collect();

        // Token overlap: only apply if query has 2+ tokens
        if query_tokens.len() >= 2 {
            let query_token_set: HashSet<_> = query_tokens.iter().copied().collect();
            results.retain(|m| {
                let doc_tokens: HashSet<_> = m.title.split_whitespace().collect();
                query_token_set.intersection(&doc_tokens).count() >= 2
            });

            if results.is_empty() {
                debug!("Poor token overlap for '{}'.", title);
                return Vec::new();
            }
        }

        // Head token alignment: only if query has 3+ tokens
        if query_tokens.len() >= 3 {
            let query_head = &query_tokens[..3];
            results.retain(|m| {
                let doc_head: Vec<_> = m.title.split_whitespace().take(3).collect();
                doc_head
                    .iter()
                    .zip(query_head)
                    .filter(|(a, b)| a == b)
                    .count()
                    >= 2
            });

            if results.is_empty() {
                debug!("Head token mismatch for '{}'.", title);
                return Vec::new();
            }
        }

        // Tail token overlap: only if query has 2+ tokens
        if query_tokens.len() >= 2 {
            let query_tail: HashSet<_> = query_tokens.iter().rev().take(2).copied().collect();
            results.retain(|m| {
                let doc_tail: HashSet<_> = m.title.split_whitespace().rev().take(2).collect();
                !query_tail.is_disjoint(&doc_tail)
            });
        }

        // Final token match: always apply if a final token exists
        if let Some(qft) = query_tokens.last() {
            results.retain(|m| m.title.split_whitespace().last() == Some(*qft));
        }

        if results.is_empty() {
            debug!("Final token mismatch for '{}'.", title);
            return Vec::new();
        }

        // Sort and normalize scores
        results.sort_by_key(|m| levenshtein(&m.title.to_lowercase(), &title.to_lowercase()));
        let top_score = results.first().map(|m| m.score).unwrap_or(1.0);
        for result in results.iter_mut() {
            result.score /= top_score;
        }

        info!(
            "Matched title: {} → imdb_id: {}, imdb_title: {}, normalized score: {}",
            title, results[0].imdb_id, results[0].title, results[0].score
        );

        results.clone()
    }
}
