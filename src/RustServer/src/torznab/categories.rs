//! Torznab category definitions.
//!
//! The full catalogue is reproduced from `TorznabCategoryTypes.cs`. Only
//! Movies / TV / XXX are exposed as top-level capabilities (matching the
//! .NET `TorznabCapabilities.Categories`), but the ID-to-label mapping
//! logic must understand every category id so that a client sending a
//! subcategory id (e.g. 2040 for Movies/HD) can be interpreted.

/// A Torznab category or sub-category.
#[derive(Debug, Clone)]
pub struct TorznabCategory {
    pub id: i32,
    pub name: &'static str,
    pub sub_categories: &'static [TorznabCategory],
}

macro_rules! cat {
    ($id:expr, $name:expr) => {
        TorznabCategory { id: $id, name: $name, sub_categories: &[] }
    };
    ($id:expr, $name:expr, $subs:expr) => {
        TorznabCategory { id: $id, name: $name, sub_categories: $subs }
    };
}

// -- Movies -----------------------------------------------------------------

pub const MOVIES_FOREIGN: TorznabCategory = cat!(2010, "Movies/Foreign");
pub const MOVIES_OTHER: TorznabCategory = cat!(2020, "Movies/Other");
pub const MOVIES_SD: TorznabCategory = cat!(2030, "Movies/SD");
pub const MOVIES_HD: TorznabCategory = cat!(2040, "Movies/HD");
pub const MOVIES_UHD: TorznabCategory = cat!(2045, "Movies/UHD");
pub const MOVIES_BLURAY: TorznabCategory = cat!(2050, "Movies/BluRay");
pub const MOVIES_3D: TorznabCategory = cat!(2060, "Movies/3D");
pub const MOVIES_DVD: TorznabCategory = cat!(2070, "Movies/DVD");
pub const MOVIES_WEBDL: TorznabCategory = cat!(2080, "Movies/WEB-DL");
pub const MOVIES_SUBCATS: &[TorznabCategory] = &[
    MOVIES_FOREIGN,
    MOVIES_OTHER,
    MOVIES_SD,
    MOVIES_HD,
    MOVIES_UHD,
    MOVIES_BLURAY,
    MOVIES_3D,
    MOVIES_DVD,
    MOVIES_WEBDL,
];
pub const MOVIES: TorznabCategory = cat!(2000, "Movies", MOVIES_SUBCATS);

// -- TV ---------------------------------------------------------------------

pub const TV_WEBDL: TorznabCategory = cat!(5010, "TV/WEB-DL");
pub const TV_FOREIGN: TorznabCategory = cat!(5020, "TV/Foreign");
pub const TV_SD: TorznabCategory = cat!(5030, "TV/SD");
pub const TV_HD: TorznabCategory = cat!(5040, "TV/HD");
pub const TV_UHD: TorznabCategory = cat!(5045, "TV/UHD");
pub const TV_OTHER: TorznabCategory = cat!(5050, "TV/Other");
pub const TV_SPORT: TorznabCategory = cat!(5060, "TV/Sport");
pub const TV_ANIME: TorznabCategory = cat!(5070, "TV/Anime");
pub const TV_DOCUMENTARY: TorznabCategory = cat!(5080, "TV/Documentary");
pub const TV_SUBCATS: &[TorznabCategory] = &[
    TV_WEBDL,
    TV_FOREIGN,
    TV_SD,
    TV_HD,
    TV_UHD,
    TV_OTHER,
    TV_SPORT,
    TV_ANIME,
    TV_DOCUMENTARY,
];
pub const TV: TorznabCategory = cat!(5000, "TV", TV_SUBCATS);

// -- XXX --------------------------------------------------------------------

pub const XXX_DVD: TorznabCategory = cat!(6010, "XXX/DVD");
pub const XXX_WMV: TorznabCategory = cat!(6020, "XXX/WMV");
pub const XXX_XVID: TorznabCategory = cat!(6030, "XXX/XviD");
pub const XXX_X264: TorznabCategory = cat!(6040, "XXX/x264");
pub const XXX_UHD: TorznabCategory = cat!(6045, "XXX/UHD");
pub const XXX_PACK: TorznabCategory = cat!(6050, "XXX/Pack");
pub const XXX_IMAGE_SET: TorznabCategory = cat!(6060, "XXX/ImageSet");
pub const XXX_OTHER: TorznabCategory = cat!(6070, "XXX/Other");
pub const XXX_SD: TorznabCategory = cat!(6080, "XXX/SD");
pub const XXX_WEBDL: TorznabCategory = cat!(6090, "XXX/WEB-DL");
pub const XXX_SUBCATS: &[TorznabCategory] = &[
    XXX_DVD,
    XXX_WMV,
    XXX_XVID,
    XXX_X264,
    XXX_UHD,
    XXX_PACK,
    XXX_IMAGE_SET,
    XXX_OTHER,
    XXX_SD,
    XXX_WEBDL,
];
pub const XXX: TorznabCategory = cat!(6000, "XXX", XXX_SUBCATS);

/// Top-level categories surfaced in the capabilities response. Ordered to
/// match `TorznabCapabilities.Categories`.
pub const EXPOSED_CATEGORIES: &[TorznabCategory] = &[MOVIES, TV, XXX];

// -- helpers ---------------------------------------------------------------

/// Look up an internal category label ("movie" / "tvSeries" / "xxx") for a
/// raw Torznab category id. Subcategory ids fold up to their parent.
pub fn internal_label_for_id(id: i32) -> Option<&'static str> {
    if id == MOVIES.id || MOVIES.sub_categories.iter().any(|c| c.id == id) {
        Some("movie")
    } else if id == TV.id || TV.sub_categories.iter().any(|c| c.id == id) {
        Some("tvSeries")
    } else if id == XXX.id || XXX.sub_categories.iter().any(|c| c.id == id) {
        Some("xxx")
    } else {
        None
    }
}

/// Internal label → Torznab category ids for emission in an `<item>`. Any
/// label not in the map defaults to Movies (matching `GetCategory` in
/// `TorznabEndpoints.cs`).
pub fn ids_for_internal_label(label: &str) -> Vec<i32> {
    match label {
        "tvSeries" => vec![TV.id],
        "xxx" => vec![XXX.id],
        // "movie" or anything else
        _ => vec![MOVIES.id],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_parent_ids() {
        assert_eq!(internal_label_for_id(2000), Some("movie"));
        assert_eq!(internal_label_for_id(5000), Some("tvSeries"));
        assert_eq!(internal_label_for_id(6000), Some("xxx"));
    }

    #[test]
    fn maps_subcategory_ids() {
        assert_eq!(internal_label_for_id(2040), Some("movie")); // Movies/HD
        assert_eq!(internal_label_for_id(5045), Some("tvSeries")); // TV/UHD
        assert_eq!(internal_label_for_id(6070), Some("xxx")); // XXX/Other
    }

    #[test]
    fn rejects_unknown_id() {
        assert_eq!(internal_label_for_id(1234), None);
    }

    #[test]
    fn ids_for_label_defaults_to_movies() {
        assert_eq!(ids_for_internal_label("unknown"), vec![MOVIES.id]);
    }
}
