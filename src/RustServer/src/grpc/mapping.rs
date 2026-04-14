//! Mapping from `parsett_rust::ParsedTitle` to the internal `proto::TorrentInfo`
//! still used by the DMM page parser. Phase 4 removed the gRPC streaming
//! helpers that used to live here; only the torrent-info mapper remains.

use crate::proto::TorrentInfo;
use crate::{proto, utils};
use parsett_rust::ParsedTitle;

pub fn map_torrent_info(
    info_hash: &str,
    original_title: &str,
    bytes: i64,
    parsed: ParsedTitle,
) -> TorrentInfo {
    let ParsedTitle {
        title,
        resolution,
        date,
        year,
        ppv,
        trash,
        adult,
        edition,
        extended,
        convert,
        hardcoded,
        proper,
        repack,
        retail,
        remastered,
        unrated,
        region,
        bitrate,
        bit_depth,
        hdr,
        audio,
        channels,
        group,
        container,
        volumes,
        seasons,
        episodes,
        episode_code,
        complete,
        dubbed,
        site,
        extension,
        subbed,
        documentary,
        upscaled,
        is_3d,
        extras,
        size: _,
        scene,
        network,
        codec,
        quality,
        languages,
    } = parsed;

    let category = assign_category(adult, &seasons, &episodes);
    let parsed_title = title;
    let normalized_title = utils::strings::normalize_title(&parsed_title);

    TorrentInfo {
        raw_title: original_title.into(),
        parsed_title,
        normalized_title,
        cleaned_parsed_title: None,
        info_hash: info_hash.into(),
        resolution,
        date,
        year,
        ppv,
        trash,
        is_adult: adult,
        edition,
        extended,
        convert,
        hardcoded,
        proper,
        repack,
        retail,
        remastered,
        unrated,
        region,
        bitrate,
        bit_depth,
        hdr,
        audio,
        channels,
        group,
        container,
        volumes,
        seasons,
        episodes,
        episode_code,
        complete,
        dubbed,
        site,
        extension,
        torrent: None,
        category,
        subbed,
        documentary,
        upscaled,
        is_3d,
        extras,
        size: Some(bytes.to_string()),
        scene,
        country: None,
        imdb_id: None,
        ingested_at: chrono::Utc::now().to_string(),
        network: network.map(|n| proto::Network::from(n) as i32),
        codec: codec.map(|c| proto::Codec::from(c) as i32),
        quality: quality.map(|q| proto::Quality::from(q) as i32),
        languages: languages
            .into_iter()
            .map(|l| proto::Language::from(l) as i32)
            .collect(),
    }
}

fn assign_category(adult: bool, seasons: &[i32], episodes: &[i32]) -> String {
    if adult {
        "xxx".to_string()
    } else if seasons.is_empty() && episodes.is_empty() {
        "movie".to_string()
    } else {
        "tvSeries".to_string()
    }
}

impl From<parsett_rust::types::Codec> for proto::Codec {
    fn from(value: parsett_rust::types::Codec) -> Self {
        proto::Codec::try_from(value as i32).unwrap_or(proto::Codec::Unknown)
    }
}

impl From<parsett_rust::types::Network> for proto::Network {
    fn from(value: parsett_rust::types::Network) -> Self {
        proto::Network::try_from(value as i32).unwrap_or(proto::Network::Unknown)
    }
}

impl From<parsett_rust::types::Language> for proto::Language {
    fn from(value: parsett_rust::types::Language) -> Self {
        proto::Language::try_from(value as i32).unwrap_or(proto::Language::LangUnknown)
    }
}

impl From<parsett_rust::types::Quality> for proto::Quality {
    fn from(value: parsett_rust::types::Quality) -> Self {
        proto::Quality::try_from(value as i32).unwrap_or(proto::Quality::Unknown)
    }
}
