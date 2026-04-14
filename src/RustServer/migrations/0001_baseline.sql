-- Phase 1 baseline migration.
--
-- This is the new canonical schema definition for Zilean. It reproduces the
-- exact final schema produced by the 15 Entity Framework Core migrations
-- that shipped with the .NET app, up to and including
-- 20250125174134_EnableUnaccent (the last EF migration).
--
-- Every statement is idempotent, so this file is safe to apply in both
-- scenarios we care about:
--
--   1. A brand new empty database. Everything is created from scratch.
--   2. A database previously migrated by the .NET app. In that case every
--      statement short-circuits (IF NOT EXISTS / OR REPLACE / conditional
--      DO block) because the objects already exist. The __EFMigrationsHistory
--      table is not touched and can be dropped manually once .NET is retired.
--
-- Constraint, index, and foreign-key names match the ones EF Core produced,
-- so existing deployments see no schema drift.


-- ---------------------------------------------------------------------------
-- Extensions
-- ---------------------------------------------------------------------------

CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS btree_gin;
CREATE EXTENSION IF NOT EXISTS btree_gist;
CREATE EXTENSION IF NOT EXISTS unaccent;


-- ---------------------------------------------------------------------------
-- ImdbFiles
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public."ImdbFiles" (
    "ImdbId"   text    NOT NULL,
    "Category" text    NULL,
    "Title"    text    NULL,
    "Adult"    boolean NOT NULL,
    "Year"     integer NOT NULL,
    CONSTRAINT "PK_ImdbFiles" PRIMARY KEY ("ImdbId")
);

CREATE UNIQUE INDEX IF NOT EXISTS "IX_ImdbFiles_ImdbId"    ON public."ImdbFiles" ("ImdbId");
CREATE INDEX        IF NOT EXISTS idx_imdb_metadata_adult    ON public."ImdbFiles" ("Adult");
CREATE INDEX        IF NOT EXISTS idx_imdb_metadata_category ON public."ImdbFiles" ("Category");
CREATE INDEX        IF NOT EXISTS idx_imdb_metadata_year     ON public."ImdbFiles" ("Year");
CREATE INDEX        IF NOT EXISTS title_gin                  ON public."ImdbFiles" USING gin ("Title" gin_trgm_ops);


-- ---------------------------------------------------------------------------
-- ImportMetadata
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public."ImportMetadata" (
    "Key"   text  NOT NULL,
    "Value" jsonb NOT NULL,
    CONSTRAINT "PK_ImportMetadata" PRIMARY KEY ("Key")
);


-- ---------------------------------------------------------------------------
-- ParsedPages
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public."ParsedPages" (
    "Page"       text    NOT NULL,
    "EntryCount" integer NOT NULL,
    CONSTRAINT "PK_ParsedPages" PRIMARY KEY ("Page")
);


-- ---------------------------------------------------------------------------
-- Torrents
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public."Torrents" (
    "InfoHash"           text                     NOT NULL,
    "RawTitle"           text                     NOT NULL,
    "ParsedTitle"        text                     NOT NULL,
    "NormalizedTitle"    text                     NOT NULL,
    "CleanedParsedTitle" text                     NOT NULL DEFAULT '',
    "Trash"              boolean                  NOT NULL,
    "Year"               integer                  NULL,
    "Resolution"         text                     NOT NULL,
    "Seasons"            integer[]                NOT NULL,
    "Episodes"           integer[]                NOT NULL,
    "Complete"           boolean                  NOT NULL,
    "Volumes"            integer[]                NOT NULL,
    "Languages"          text[]                   NOT NULL,
    "Quality"            text                     NULL,
    "Hdr"                text[]                   NOT NULL,
    "Codec"              text                     NULL,
    "Audio"              text[]                   NOT NULL,
    "Channels"           text[]                   NOT NULL,
    "Dubbed"             boolean                  NOT NULL,
    "Subbed"             boolean                  NOT NULL,
    "Date"               text                     NULL,
    "Group"              text                     NULL,
    "Edition"            text                     NULL,
    "BitDepth"           text                     NULL,
    "Bitrate"            text                     NULL,
    "Network"            text                     NULL,
    "Extended"           boolean                  NOT NULL,
    "Converted"          boolean                  NOT NULL,
    "Hardcoded"          boolean                  NOT NULL,
    "Region"             text                     NULL,
    "Ppv"                boolean                  NOT NULL,
    "Is3d"               boolean                  NOT NULL,
    "Site"               text                     NULL,
    "Size"               text                     NULL,
    "Proper"             boolean                  NOT NULL,
    "Repack"             boolean                  NOT NULL,
    "Retail"             boolean                  NOT NULL,
    "Upscaled"           boolean                  NOT NULL,
    "Remastered"         boolean                  NOT NULL,
    "Unrated"            boolean                  NOT NULL,
    "Documentary"        boolean                  NOT NULL,
    "EpisodeCode"        text                     NULL,
    "Country"            text                     NULL,
    "Container"          text                     NULL,
    "Extension"          text                     NULL,
    "Torrent"            boolean                  NOT NULL,
    "Category"           text                     NOT NULL,
    "ImdbId"             text                     NULL,
    "IsAdult"            boolean                  NOT NULL DEFAULT false,
    "IngestedAt"         timestamp with time zone NOT NULL DEFAULT (now() at time zone 'utc'),
    CONSTRAINT "PK_Torrents" PRIMARY KEY ("InfoHash")
);

-- Guard against databases that were created between EF migrations. On a fresh
-- install these are no-ops; they only matter for an install that stopped part
-- way through the EF sequence.
ALTER TABLE public."Torrents"
    ADD COLUMN IF NOT EXISTS "CleanedParsedTitle" text NOT NULL DEFAULT '';
ALTER TABLE public."Torrents"
    ADD COLUMN IF NOT EXISTS "IsAdult" boolean NOT NULL DEFAULT false;
ALTER TABLE public."Torrents"
    ADD COLUMN IF NOT EXISTS "IngestedAt" timestamp with time zone NOT NULL DEFAULT (now() at time zone 'utc');

-- Torrents -> ImdbFiles foreign key. PostgreSQL has no IF NOT EXISTS form for
-- ALTER TABLE ADD CONSTRAINT, so check pg_constraint first.
DO $do$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'FK_Torrents_ImdbFiles_ImdbId'
    ) THEN
        ALTER TABLE public."Torrents"
            ADD CONSTRAINT "FK_Torrents_ImdbFiles_ImdbId"
            FOREIGN KEY ("ImdbId") REFERENCES public."ImdbFiles" ("ImdbId");
    END IF;
END
$do$;

-- Torrents indexes. Names match EF exactly.
CREATE UNIQUE INDEX IF NOT EXISTS "IX_Torrents_InfoHash"        ON public."Torrents" ("InfoHash");
CREATE INDEX        IF NOT EXISTS idx_torrents_imdbid            ON public."Torrents" ("ImdbId");
CREATE INDEX        IF NOT EXISTS idx_torrents_infohash          ON public."Torrents" ("InfoHash");
CREATE INDEX        IF NOT EXISTS torrents_title_gin             ON public."Torrents" USING gin ("ParsedTitle" gin_trgm_ops);
CREATE INDEX        IF NOT EXISTS idx_cleaned_parsed_title_trgm  ON public."Torrents" USING gin ("CleanedParsedTitle" gin_trgm_ops);
CREATE INDEX        IF NOT EXISTS idx_seasons_gin                ON public."Torrents" USING gin ("Seasons");
CREATE INDEX        IF NOT EXISTS idx_episodes_gin               ON public."Torrents" USING gin ("Episodes");
CREATE INDEX        IF NOT EXISTS idx_languages_gin              ON public."Torrents" USING gin ("Languages");
CREATE INDEX        IF NOT EXISTS idx_year                       ON public."Torrents" ("Year");
CREATE INDEX        IF NOT EXISTS idx_torrents_isadult           ON public."Torrents" ("IsAdult");
CREATE INDEX        IF NOT EXISTS idx_torrents_trash             ON public."Torrents" ("Trash");
CREATE INDEX        IF NOT EXISTS idx_ingested_at                ON public."Torrents" ("IngestedAt" DESC);
CREATE INDEX        IF NOT EXISTS idx_infohash_length_40         ON public."Torrents" (length("InfoHash"));


-- ---------------------------------------------------------------------------
-- BlacklistedItems
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public."BlacklistedItems" (
    "InfoHash"      text                     NOT NULL,
    "Reason"        text                     NOT NULL,
    "BlacklistedAt" timestamp with time zone NOT NULL DEFAULT (now() at time zone 'utc'),
    CONSTRAINT "PK_BlacklistedItems" PRIMARY KEY ("InfoHash")
);

CREATE UNIQUE INDEX IF NOT EXISTS "IX_BlacklistedItems_InfoHash" ON public."BlacklistedItems" ("InfoHash");


-- ---------------------------------------------------------------------------
-- search_torrents_meta (V5)
--
-- Body copied verbatim from Zilean.Database.Functions.SearchTorrentsMetaV5.
-- CREATE OR REPLACE, so running this on an existing install just replaces
-- the identical function body with itself.
-- ---------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION search_torrents_meta(
    query                TEXT DEFAULT NULL,
    season               INT  DEFAULT NULL,
    episode              INT  DEFAULT NULL,
    year                 INT  DEFAULT NULL,
    language             TEXT DEFAULT NULL,
    resolution           TEXT DEFAULT NULL,
    imdbId               TEXT DEFAULT NULL,
    limit_param          INT  DEFAULT 20,
    category             TEXT DEFAULT NULL,
    similarity_threshold REAL DEFAULT 0.85
)
RETURNS TABLE(
    "InfoHash"        TEXT,
    "Resolution"      TEXT,
    "Year"            INT,
    "Remastered"      BOOLEAN,
    "Codec"           TEXT,
    "Audio"           TEXT[],
    "Quality"         TEXT,
    "Episodes"        INT[],
    "Seasons"         INT[],
    "Languages"       TEXT[],
    "ParsedTitle"     TEXT,
    "NormalizedTitle" TEXT,
    "RawTitle"        TEXT,
    "Size"            TEXT,
    "Category"        TEXT,
    "Complete"        BOOLEAN,
    "Volumes"         INT[],
    "Hdr"             TEXT[],
    "Channels"        TEXT[],
    "Dubbed"          BOOLEAN,
    "Subbed"          BOOLEAN,
    "Edition"         TEXT,
    "BitDepth"        TEXT,
    "Bitrate"         TEXT,
    "Network"         TEXT,
    "Extended"        BOOLEAN,
    "Converted"       BOOLEAN,
    "Hardcoded"       BOOLEAN,
    "Region"          TEXT,
    "Ppv"             BOOLEAN,
    "Is3d"            BOOLEAN,
    "Site"            TEXT,
    "Proper"          BOOLEAN,
    "Repack"          BOOLEAN,
    "Retail"          BOOLEAN,
    "Upscaled"        BOOLEAN,
    "Unrated"         BOOLEAN,
    "Documentary"     BOOLEAN,
    "EpisodeCode"     TEXT,
    "Country"         TEXT,
    "Container"       TEXT,
    "Extension"       TEXT,
    "Torrent"         BOOLEAN,
    "Score"           REAL,
    "ImdbId"          TEXT,
    "ImdbCategory"    TEXT,
    "ImdbTitle"       TEXT,
    "ImdbYear"        INT,
    "ImdbAdult"       BOOLEAN,
    "IngestedAt"      TIMESTAMPTZ
) AS $func$
BEGIN
    EXECUTE format('SET pg_trgm.similarity_threshold = %L', similarity_threshold);

    RETURN QUERY
    SELECT
        t."InfoHash",
        t."Resolution",
        t."Year",
        t."Remastered",
        t."Codec",
        t."Audio",
        t."Quality",
        t."Episodes",
        t."Seasons",
        t."Languages",
        t."ParsedTitle",
        t."NormalizedTitle",
        t."RawTitle",
        t."Size",
        t."Category",
        t."Complete",
        t."Volumes",
        t."Hdr",
        t."Channels",
        t."Dubbed",
        t."Subbed",
        t."Edition",
        t."BitDepth",
        t."Bitrate",
        t."Network",
        t."Extended",
        t."Converted",
        t."Hardcoded",
        t."Region",
        t."Ppv",
        t."Is3d",
        t."Site",
        t."Proper",
        t."Repack",
        t."Retail",
        t."Upscaled",
        t."Unrated",
        t."Documentary",
        t."EpisodeCode",
        t."Country",
        t."Container",
        t."Extension",
        t."Torrent",
        similarity(t."CleanedParsedTitle", query) AS "Score",
        t."ImdbId",
        i."Category" AS "ImdbCategory",
        i."Title"    AS "ImdbTitle",
        i."Year"     AS "ImdbYear",
        i."Adult"    AS "ImdbAdult",
        t."IngestedAt"
    FROM
        public."Torrents" t
    LEFT JOIN
        public."ImdbFiles" i ON t."ImdbId" = i."ImdbId"
    WHERE
        Length(t."InfoHash") = 40
    AND
        (category IS NULL OR t."Category" = category)
    AND
        (query IS NULL OR t."CleanedParsedTitle" % query)
    AND (imdbId IS NULL OR t."ImdbId" = imdbId)
    AND (season IS NULL OR season = ANY(t."Seasons"))
    AND (
        (episode IS NULL AND season IS NOT NULL)
        OR
        (
            episode IS NOT NULL AND
            season IS NOT NULL AND
            (episode = ANY(t."Episodes") OR t."Episodes" IS NULL OR t."Episodes" = '{}')
        )
        OR (season IS NULL AND episode IS NULL)
    )
    AND (year IS NULL OR t."Year" BETWEEN year - 1 AND year + 1)
    AND (language IS NULL OR language = ANY(t."Languages"))
    AND (resolution IS NULL OR resolution = t."Resolution")
    ORDER BY
        "Score" DESC,
        "IngestedAt" DESC
    LIMIT
        limit_param;
END;
$func$ LANGUAGE plpgsql;


-- ---------------------------------------------------------------------------
-- search_imdb_meta (V3)
--
-- Body copied verbatim from Zilean.Database.Functions.SearchImdbProcedureV3.
-- ---------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION search_imdb_meta(
    search_term          TEXT,
    category_param       TEXT DEFAULT NULL,
    year_param           INT  DEFAULT NULL,
    limit_param          INT  DEFAULT 10,
    similarity_threshold REAL DEFAULT 0.85
)
RETURNS TABLE(imdb_id TEXT, title TEXT, category TEXT, year INT, score REAL) AS $func$
BEGIN
    EXECUTE format('SET pg_trgm.similarity_threshold = %L', similarity_threshold);
    RETURN QUERY
        SELECT "ImdbId", "Title", "Category", "Year", similarity("Title", search_term) AS score
        FROM public."ImdbFiles"
        WHERE ("Title" % search_term)
          AND ("Adult" = FALSE)
          AND (
               category_param IS NULL
               OR (
                   category_param = 'movie' AND "Category" IN ('movie', 'tvMovie')
               )
               OR (
                   category_param = 'tvSeries' AND "Category" IN ('tvSeries', 'tvShort', 'tvMiniSeries', 'tvSpecial')
               )
               OR (
                   category_param NOT IN ('movie', 'tvSeries') AND "Category" = category_param
               )
           )
          AND (year_param IS NULL OR "Year" BETWEEN year_param - 1 AND year_param + 1)
        ORDER BY score DESC
        LIMIT limit_param;
END;
$func$ LANGUAGE plpgsql;
