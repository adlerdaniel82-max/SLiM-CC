use crate::error::{SlimError, SlimResult};
use crate::models::{
    AnalyzeNexusCollectionRequest, NexusApiStatus, NexusCollectionAnalysis, NexusCollectionItem,
    NexusModCache, NexusModLink, NexusRequirement, NexusRequirementStatus, NexusSourceLink,
    NexusSyncResult, UpdateNexusModLinkRequest,
};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::fs;
use std::path::Path;

const NEXUS_API_BASE: &str = "https://api.nexusmods.com/v3";
const NEXUS_LEGACY_API_BASE: &str = "https://api.nexusmods.com/v1";

pub fn list_nexus_mod_links(conn: &Connection, instance_id: &str) -> SlimResult<Vec<NexusModLink>> {
    let mut stmt = conn.prepare(
        "SELECT n.mod_id, n.game_domain, n.nexus_mod_id, n.nexus_file_id, n.source_url, n.last_checked_at
         FROM nexus_mod_links n
         JOIN mods m ON m.id = n.mod_id
         WHERE m.instance_id = ?1
         ORDER BY m.name ASC, n.game_domain ASC, n.nexus_mod_id ASC",
    )?;
    let rows = stmt.query_map(params![instance_id], |row| {
        Ok(NexusModLink {
            mod_id: row.get(0)?,
            game_domain: row.get(1)?,
            nexus_mod_id: row.get(2)?,
            nexus_file_id: row.get(3)?,
            source_url: row.get(4)?,
            last_checked_at: row.get(5)?,
        })
    })?;

    let mut links = Vec::new();
    for row in rows {
        links.push(row?);
    }
    Ok(links)
}

pub fn upsert_nexus_mod_link(
    conn: &Connection,
    request: UpdateNexusModLinkRequest,
) -> SlimResult<Option<NexusModLink>> {
    let game_domain = request.game_domain.trim().to_lowercase();
    let source_url = request.source_url.trim().to_string();
    if game_domain.is_empty() && request.nexus_mod_id.is_none() && source_url.is_empty() {
        conn.execute(
            "DELETE FROM nexus_mod_links WHERE mod_id = ?1",
            params![&request.mod_id],
        )?;
        return Ok(None);
    }

    let nexus_mod_id = request
        .nexus_mod_id
        .filter(|value| *value > 0)
        .ok_or_else(|| SlimError::InvalidPath("Nexus mod id must be a positive number".into()))?;
    let nexus_file_id = request.nexus_file_id.filter(|value| *value > 0);
    if game_domain.is_empty() {
        return Err(SlimError::InvalidPath(
            "Nexus game domain is required when Nexus mod id is set".into(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO nexus_mod_links (
             mod_id, game_domain, nexus_mod_id, nexus_file_id, source_url, last_checked_at, created_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?6)
         ON CONFLICT(mod_id) DO UPDATE SET
             game_domain = excluded.game_domain,
             nexus_mod_id = excluded.nexus_mod_id,
             nexus_file_id = excluded.nexus_file_id,
             source_url = excluded.source_url,
             updated_at = excluded.updated_at",
        params![
            &request.mod_id,
            game_domain,
            nexus_mod_id,
            nexus_file_id,
            source_url,
            &now
        ],
    )?;

    get_nexus_mod_link(conn, &request.mod_id).map(Some)
}

pub fn get_nexus_mod_link(conn: &Connection, mod_id: &str) -> SlimResult<NexusModLink> {
    Ok(conn.query_row(
        "SELECT mod_id, game_domain, nexus_mod_id, nexus_file_id, source_url, last_checked_at
         FROM nexus_mod_links
         WHERE mod_id = ?1",
        params![mod_id],
        |row| {
            Ok(NexusModLink {
                mod_id: row.get(0)?,
                game_domain: row.get(1)?,
                nexus_mod_id: row.get(2)?,
                nexus_file_id: row.get(3)?,
                source_url: row.get(4)?,
                last_checked_at: row.get(5)?,
            })
        },
    )?)
}

pub fn list_nexus_requirements(
    conn: &Connection,
    mod_id: &str,
) -> SlimResult<Vec<NexusRequirement>> {
    let mut stmt = conn.prepare(
        "SELECT id, mod_id, required_game_domain, required_nexus_mod_id, required_name,
                requirement_type, source, notes, fetched_at
         FROM nexus_requirements
         WHERE mod_id = ?1
         ORDER BY required_name ASC, id ASC",
    )?;
    let rows = stmt.query_map(params![mod_id], |row| {
        Ok(NexusRequirement {
            id: row.get(0)?,
            mod_id: row.get(1)?,
            required_game_domain: row.get(2)?,
            required_nexus_mod_id: row.get(3)?,
            required_name: row.get(4)?,
            requirement_type: row.get(5)?,
            source: row.get(6)?,
            notes: row.get(7)?,
            fetched_at: row.get(8)?,
        })
    })?;

    let mut requirements = Vec::new();
    for row in rows {
        requirements.push(row?);
    }
    Ok(requirements)
}

pub fn list_nexus_requirement_status(
    conn: &Connection,
    mod_id: &str,
) -> SlimResult<Vec<NexusRequirementStatus>> {
    let instance_id = mod_instance_id(conn, mod_id)?;
    let requirements = list_nexus_requirements(conn, mod_id)?;
    let mut statuses = Vec::new();
    for requirement in requirements {
        let matched = find_matching_requirement_mod(conn, &instance_id, mod_id, &requirement)?;
        let satisfied = matched.is_some();
        let (matched_mod_id, matched_mod_name) = matched.unwrap_or((None, None));
        statuses.push(NexusRequirementStatus {
            id: requirement.id,
            mod_id: requirement.mod_id,
            required_game_domain: requirement.required_game_domain,
            required_nexus_mod_id: requirement.required_nexus_mod_id,
            required_name: requirement.required_name,
            requirement_type: requirement.requirement_type,
            source: requirement.source,
            notes: requirement.notes,
            fetched_at: requirement.fetched_at,
            matched_mod_id,
            matched_mod_name,
            satisfied,
            status: if satisfied { "satisfied" } else { "missing" }.into(),
        });
    }
    Ok(statuses)
}

pub fn list_nexus_mod_cache(
    conn: &Connection,
    instance_id: &str,
) -> SlimResult<Vec<NexusModCache>> {
    let mut stmt = conn.prepare(
        "SELECT c.mod_id, c.game_domain, c.nexus_mod_id, c.name, c.version, c.updated_time,
                c.endorsement_count, c.mod_downloads, c.fetched_at
         FROM nexus_mod_cache c
         JOIN mods m ON m.id = c.mod_id
         WHERE m.instance_id = ?1
         ORDER BY m.name ASC",
    )?;
    let rows = stmt.query_map(params![instance_id], row_to_nexus_mod_cache)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn mod_instance_id(conn: &Connection, mod_id: &str) -> SlimResult<String> {
    Ok(conn.query_row(
        "SELECT instance_id FROM mods WHERE id = ?1",
        params![mod_id],
        |row| row.get(0),
    )?)
}

fn find_matching_requirement_mod(
    conn: &Connection,
    instance_id: &str,
    source_mod_id: &str,
    requirement: &NexusRequirement,
) -> SlimResult<Option<(Option<String>, Option<String>)>> {
    if let Some(required_nexus_mod_id) = requirement.required_nexus_mod_id {
        let mut stmt = conn.prepare(
            "SELECT m.id, m.name
             FROM nexus_mod_links n
             JOIN mods m ON m.id = n.mod_id
             WHERE m.instance_id = ?1
               AND m.id <> ?2
               AND LOWER(n.game_domain) = LOWER(?3)
               AND n.nexus_mod_id = ?4
             ORDER BY m.name ASC
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![
            instance_id,
            source_mod_id,
            &requirement.required_game_domain,
            required_nexus_mod_id
        ])?;
        if let Some(row) = rows.next()? {
            return Ok(Some((Some(row.get(0)?), Some(row.get(1)?))));
        }
    }

    let mut stmt = conn.prepare(
        "SELECT id, name
         FROM mods
         WHERE instance_id = ?1
           AND id <> ?2
           AND LOWER(name) = LOWER(?3)
         ORDER BY name ASC
         LIMIT 1",
    )?;
    let mut rows = stmt.query(params![
        instance_id,
        source_mod_id,
        requirement.required_name.trim()
    ])?;
    if let Some(row) = rows.next()? {
        Ok(Some((Some(row.get(0)?), Some(row.get(1)?))))
    } else {
        Ok(None)
    }
}

pub fn validate_nexus_api(conn: &Connection) -> SlimResult<NexusApiStatus> {
    let Some(api_key) = crate::settings::load_nexus_api_key(conn)? else {
        return Ok(NexusApiStatus {
            configured: false,
            user_id: None,
            name: None,
            is_premium: None,
            is_supporter: None,
            hourly_remaining: None,
            daily_remaining: None,
            message: "Nexus API key not configured. Add it under Settings.".into(),
        });
    };

    let response = nexus_get(&api_key, "/games/skyrimspecialedition/mods/12604")?;
    let body = response.body;
    Ok(NexusApiStatus {
        configured: true,
        user_id: None,
        name: body
            .pointer("/data/name")
            .and_then(Value::as_str)
            .map(str::to_string),
        is_premium: None,
        is_supporter: None,
        hourly_remaining: response.hourly_remaining,
        daily_remaining: response.daily_remaining,
        message: "Nexus API key is valid for v3 metadata requests.".into(),
    })
}

pub fn parse_nexus_source_link(raw_url: &str) -> Option<NexusSourceLink> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.to_lowercase().starts_with("nxm://") {
        return parse_nxm_source_link(trimmed);
    }

    parse_nexus_page_link(trimmed)
}

pub fn analyze_nexus_collection(
    conn: &Connection,
    request: AnalyzeNexusCollectionRequest,
) -> SlimResult<NexusCollectionAnalysis> {
    let source_url = request.collection_url.trim();
    if source_url.is_empty() {
        return Err(SlimError::InvalidPath(
            "Nexus collection URL must not be empty".into(),
        ));
    }

    let api_key = crate::settings::load_nexus_api_key(conn)?.ok_or_else(|| {
        SlimError::InvalidPath("Nexus API key is required for collection analysis".into())
    })?;
    let (slug, revision) = parse_collection_reference(source_url)?;
    let body = fetch_collection_revision(&api_key, &slug, revision)?;
    let revision_data = body.pointer("/data/collectionRevision").ok_or_else(|| {
        SlimError::Process("Nexus collection API returned no collection revision".into())
    })?;
    let collection = revision_data.get("collection").unwrap_or(&Value::Null);
    let game_domain = collection
        .pointer("/game/domainName")
        .and_then(Value::as_str)
        .unwrap_or("skyrimspecialedition");
    let mut items = Vec::new();
    for entry in revision_data
        .get("modFiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let file = entry.get("file").unwrap_or(&Value::Null);
        let mod_data = file.get("mod").unwrap_or(&Value::Null);
        let mod_id = file
            .get("modId")
            .or_else(|| mod_data.get("modId"))
            .and_then(as_i64_flexible);
        let file_id = entry
            .get("fileId")
            .or_else(|| file.get("fileId"))
            .and_then(as_i64_flexible);
        let domain = mod_data
            .pointer("/game/domainName")
            .and_then(Value::as_str)
            .unwrap_or(game_domain);
        if let (Some(mod_id), Some(file_id)) = (mod_id, file_id) {
            items.push(NexusCollectionItem {
                title: mod_data
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| file.get("name").and_then(Value::as_str))
                    .unwrap_or("Nexus Mod")
                    .to_string(),
                url: format!(
                    "https://www.nexusmods.com/{domain}/mods/{mod_id}?tab=files&file_id={file_id}"
                ),
                game_domain: Some(domain.to_string()),
                nexus_mod_id: Some(mod_id),
                nexus_file_id: Some(file_id),
                item_type: if entry
                    .get("optional")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    "optional".into()
                } else {
                    "required".into()
                },
            });
        }
    }
    for external in revision_data
        .get("externalResources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(url) = external.get("resourceUrl").and_then(Value::as_str) {
            items.push(NexusCollectionItem {
                title: external
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Externe Voraussetzung")
                    .to_string(),
                url: url.to_string(),
                game_domain: None,
                nexus_mod_id: None,
                nexus_file_id: None,
                item_type: "external_requirement".into(),
            });
        }
    }
    let unresolved_links = if items.is_empty() {
        vec![source_url.to_string()]
    } else {
        Vec::new()
    };

    Ok(NexusCollectionAnalysis {
        source_url: source_url.to_string(),
        collection_title: collection
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        collection_description: collection
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_string),
        resolver: "nexus-graphql-v2".into(),
        items,
        unresolved_links,
        notes: vec![format!(
            "Collection revision {} was resolved through the Nexus GraphQL API.",
            revision_data
                .get("revisionNumber")
                .and_then(Value::as_i64)
                .unwrap_or_default()
        )],
    })
}

pub fn download_nexus_file(
    conn: &Connection,
    request: crate::models::NexusDownloadRequest,
) -> SlimResult<crate::models::NexusDownloadResult> {
    let settings = crate::settings::load_settings(conn)?;
    let download_root = settings
        .mod_download_path
        .ok_or_else(|| SlimError::InvalidPath("Mod download folder is not configured".into()))?;
    fs::create_dir_all(&download_root)?;
    if let Some(uri) = validated_nexus_cdn_url(&request.source_url) {
        return download_from_nexus_cdn(&download_root, uri, None);
    }
    let api_key = crate::settings::load_nexus_api_key(conn)?
        .ok_or_else(|| SlimError::InvalidPath("Nexus API key is not configured".into()))?;
    let link = parse_nexus_source_link(&request.source_url)
        .ok_or_else(|| SlimError::InvalidPath("Unsupported Nexus download link".into()))?;
    let file_id = link
        .nexus_file_id
        .ok_or_else(|| SlimError::InvalidPath("Nexus link does not contain a file ID".into()))?;
    let (key, expires) = nxm_download_credentials(&request.source_url);
    let suffix = match (key, expires) {
        (Some(key), Some(expires)) => format!("?key={}&expires={}", url_path(&key), expires),
        _ => String::new(),
    };
    let links = nexus_v1_get(
        &api_key,
        &format!(
            "/games/{}/mods/{}/files/{}/download_link{}",
            url_path(&link.game_domain),
            link.nexus_mod_id,
            file_id,
            suffix
        ),
    )?
    .body;
    let uri = links.as_array().and_then(|values| values.first()).and_then(|value| value.get("URI").or_else(|| value.get("uri"))).and_then(Value::as_str)
        .ok_or_else(|| SlimError::Process("Nexus returned no download mirror. Non-premium users must use an nxm link from Download with Manager.".into()))?;
    let metadata = nexus_v1_get(
        &api_key,
        &format!(
            "/games/{}/mods/{}/files/{}",
            url_path(&link.game_domain),
            link.nexus_mod_id,
            file_id
        ),
    )?
    .body;
    let raw_name = metadata
        .get("file_name")
        .or_else(|| metadata.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("nexus-download.zip");
    let file_name = Path::new(raw_name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("nexus-download.zip")
        .to_string();
    download_from_nexus_cdn(&download_root, uri.to_string(), Some((file_name, link)))
}

fn download_from_nexus_cdn(
    download_root: &Path,
    uri: String,
    metadata: Option<(String, NexusSourceLink)>,
) -> SlimResult<crate::models::NexusDownloadResult> {
    let requested_url = validated_nexus_cdn_url(&uri)
        .ok_or_else(|| SlimError::Safety("refusing download outside the Nexus CDN".into()))?;
    let mut response = reqwest::blocking::Client::new()
        .get(requested_url)
        .header(
            "User-Agent",
            format!("SLiM-CC/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()?;
    if !response.status().is_success() {
        return Err(SlimError::Process(format!(
            "Nexus CDN download failed ({})",
            response.status()
        )));
    }
    if validated_nexus_cdn_url(response.url().as_str()).is_none() {
        return Err(SlimError::Safety(
            "Nexus CDN redirected to an untrusted host".into(),
        ));
    }
    let header_name = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(content_disposition_file_name);
    let file_name = metadata
        .as_ref()
        .map(|(name, _)| name.clone())
        .or(header_name)
        .or_else(|| {
            response
                .url()
                .path_segments()?
                .next_back()
                .map(str::to_string)
        })
        .and_then(|name| safe_download_file_name(&name))
        .unwrap_or_else(|| "nexus-download.bin".into());
    let destination = download_root.join(&file_name);
    let partial = download_root.join(format!(".{file_name}.part"));
    let mut output = fs::File::create(&partial)?;
    let bytes_written = response.copy_to(&mut output)?;
    output.sync_all()?;
    fs::rename(&partial, &destination)?;
    Ok(crate::models::NexusDownloadResult {
        path: destination,
        file_name,
        bytes_written,
        game_domain: metadata.as_ref().map(|(_, link)| link.game_domain.clone()),
        nexus_mod_id: metadata.as_ref().map(|(_, link)| link.nexus_mod_id),
        nexus_file_id: metadata.as_ref().and_then(|(_, link)| link.nexus_file_id),
    })
}

fn validated_nexus_cdn_url(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    (url.scheme() == "https" && (host == "nexus-cdn.com" || host.ends_with(".nexus-cdn.com")))
        .then(|| url.to_string())
}

fn content_disposition_file_name(value: &str) -> Option<String> {
    value.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        name.eq_ignore_ascii_case("filename")
            .then(|| value.trim().trim_matches(['"', '\'']).to_string())
    })
}

fn safe_download_file_name(raw: &str) -> Option<String> {
    Path::new(raw)
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "." && *value != "..")
        .map(str::to_string)
}

fn parse_collection_reference(url: &str) -> SlimResult<(String, Option<i64>)> {
    let path = strip_query_and_fragment(url);
    let slug = path
        .split('/')
        .collect::<Vec<_>>()
        .windows(2)
        .find(|parts| parts[0].eq_ignore_ascii_case("collections"))
        .map(|parts| parts[1].to_string())
        .filter(|slug| !slug.is_empty())
        .ok_or_else(|| SlimError::InvalidPath("invalid Nexus collection URL".into()))?;
    let revision = url
        .split_once('?')
        .map(|(_, query)| query)
        .and_then(|query| {
            query.split('&').find_map(|part| {
                part.split_once('=')
                    .filter(|(key, _)| *key == "revision")
                    .and_then(|(_, value)| value.parse().ok())
            })
        });
    Ok((slug, revision))
}

fn fetch_collection_revision(
    api_key: &str,
    slug: &str,
    revision: Option<i64>,
) -> SlimResult<Value> {
    let (query, variables) = if let Some(revision) = revision {
        ("query collectionRevision($slug:String!,$revision:Int!,$viewAdultContent:Boolean){collectionRevision(slug:$slug,revision:$revision,viewAdultContent:$viewAdultContent){revisionNumber collection{name summary game{domainName}} modFiles{optional fileId file{fileId modId name mod{modId name game{domainName}}}} externalResources{name resourceUrl optional}}}", serde_json::json!({"slug":slug,"revision":revision,"viewAdultContent":true}))
    } else {
        ("query collectionRevision($slug:String!,$viewAdultContent:Boolean){collectionRevision(slug:$slug,viewAdultContent:$viewAdultContent){revisionNumber collection{name summary game{domainName}} modFiles{optional fileId file{fileId modId name mod{modId name game{domainName}}}} externalResources{name resourceUrl optional}}}", serde_json::json!({"slug":slug,"viewAdultContent":true}))
    };
    let response = reqwest::blocking::Client::new()
        .post("https://api.nexusmods.com/v2/graphql")
        .header("apikey", api_key)
        .header("Application-Name", "SLiM-CC")
        .header("Application-Version", env!("CARGO_PKG_VERSION"))
        .json(&serde_json::json!({"query":query,"variables":variables}))
        .send()?;
    let status = response.status();
    let body: Value = response.json()?;
    if !status.is_success() || body.get("errors").is_some() {
        return Err(SlimError::Process(format!(
            "Nexus collection API failed ({status}): {}",
            body.get("errors").unwrap_or(&body)
        )));
    }
    Ok(body)
}

fn nxm_download_credentials(url: &str) -> (Option<String>, Option<i64>) {
    let query = url.split_once('?').map(|(_, query)| query).unwrap_or("");
    let key = query.split('&').find_map(|part| {
        part.split_once('=')
            .filter(|(name, _)| *name == "key")
            .map(|(_, value)| value.to_string())
    });
    let expires = query.split('&').find_map(|part| {
        part.split_once('=')
            .filter(|(name, _)| *name == "expires")
            .and_then(|(_, value)| value.parse().ok())
    });
    (key, expires)
}

pub fn sync_nexus_mod(conn: &Connection, mod_id: &str) -> SlimResult<NexusSyncResult> {
    let api_key = crate::settings::load_nexus_api_key(conn)?.ok_or_else(|| {
        SlimError::InvalidPath("Nexus API key not configured. Add it under Settings.".into())
    })?;
    let link = get_nexus_mod_link(conn, mod_id)?;
    let mod_response = nexus_get(
        &api_key,
        &format!(
            "/games/{}/mods/{}",
            url_path(&link.game_domain),
            link.nexus_mod_id
        ),
    )?;
    let now = Utc::now().to_rfc3339();
    let mod_cache = store_nexus_mod_cache(conn, &link, &mod_response.body, &now)?;
    let (files_cached, requirements_cached, file_hourly, file_daily) =
        if let Some(file_id) = link.nexus_file_id {
            let file_response = nexus_get(
                &api_key,
                &format!(
                    "/games/{}/mod-files/{}",
                    url_path(&link.game_domain),
                    file_id
                ),
            )?;
            let global_file_id = file_response
                .body
                .pointer("/data/id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let files_cached = store_nexus_file_cache(conn, mod_id, &file_response.body, &now)?;
            let dependency_response = global_file_id.as_deref().and_then(|id| {
                nexus_get(
                    &api_key,
                    &format!("/mod-files/{}/dependencies/materialized", url_path(id)),
                )
                .ok()
            });
            let requirements_cached = if let Some(response) = &dependency_response {
                store_nexus_requirements(conn, mod_id, &link.game_domain, &response.body, &now)?
            } else {
                0
            };
            (
                files_cached,
                requirements_cached,
                dependency_response
                    .as_ref()
                    .and_then(|response| response.hourly_remaining)
                    .or(file_response.hourly_remaining),
                dependency_response
                    .as_ref()
                    .and_then(|response| response.daily_remaining)
                    .or(file_response.daily_remaining),
            )
        } else {
            conn.execute(
                "DELETE FROM nexus_file_cache WHERE mod_id = ?1",
                params![mod_id],
            )?;
            conn.execute(
                "DELETE FROM nexus_requirements WHERE mod_id = ?1 AND source = 'nexus'",
                params![mod_id],
            )?;
            (0, 0, None, None)
        };

    conn.execute(
        "UPDATE nexus_mod_links
         SET last_checked_at = ?2, updated_at = ?2
         WHERE mod_id = ?1",
        params![mod_id, &now],
    )?;

    Ok(NexusSyncResult {
        link: get_nexus_mod_link(conn, mod_id)?,
        mod_cache,
        files_cached,
        requirements_cached,
        hourly_remaining: file_hourly.or(mod_response.hourly_remaining),
        daily_remaining: file_daily.or(mod_response.daily_remaining),
    })
}

fn parse_nxm_source_link(raw_url: &str) -> Option<NexusSourceLink> {
    let without_scheme = raw_url.get(6..)?;
    let path = strip_query_and_fragment(without_scheme);
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 5 || parts.get(1)? != &"mods" || parts.get(3)? != &"files" {
        return None;
    }

    let game_domain = parts.first()?.trim().to_lowercase();
    let nexus_mod_id = parts.get(2)?.parse::<i64>().ok().filter(|id| *id > 0)?;
    let nexus_file_id = parts.get(4)?.parse::<i64>().ok().filter(|id| *id > 0);
    let file_segment = nexus_file_id?;
    Some(NexusSourceLink {
        sanitized_url: format!("nxm://{game_domain}/mods/{nexus_mod_id}/files/{file_segment}"),
        game_domain,
        nexus_mod_id,
        nexus_file_id,
    })
}

fn parse_nexus_page_link(raw_url: &str) -> Option<NexusSourceLink> {
    let after_scheme = raw_url
        .strip_prefix("https://")
        .or_else(|| raw_url.strip_prefix("http://"))?;
    let (host, rest) = after_scheme.split_once('/')?;
    if !host.eq_ignore_ascii_case("www.nexusmods.com")
        && !host.eq_ignore_ascii_case("nexusmods.com")
    {
        return None;
    }

    let (path, query) = split_query(rest);
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 3 || parts.get(1)? != &"mods" {
        return None;
    }

    let game_domain = parts.first()?.trim().to_lowercase();
    let nexus_mod_id = parts.get(2)?.parse::<i64>().ok().filter(|id| *id > 0)?;
    let nexus_file_id = query.and_then(extract_file_id_query_param);
    let sanitized_url = if let Some(file_id) = nexus_file_id {
        format!("https://www.nexusmods.com/{game_domain}/mods/{nexus_mod_id}?file_id={file_id}")
    } else {
        format!("https://www.nexusmods.com/{game_domain}/mods/{nexus_mod_id}")
    };

    Some(NexusSourceLink {
        game_domain,
        nexus_mod_id,
        nexus_file_id,
        sanitized_url,
    })
}

fn strip_query_and_fragment(value: &str) -> &str {
    value
        .split_once(['?', '#'])
        .map(|(path, _)| path)
        .unwrap_or(value)
}

fn split_query(value: &str) -> (&str, Option<&str>) {
    let without_fragment = value.split_once('#').map(|(path, _)| path).unwrap_or(value);
    if let Some((path, query)) = without_fragment.split_once('?') {
        (path, Some(query))
    } else {
        (without_fragment, None)
    }
}

fn extract_file_id_query_param(query: &str) -> Option<i64> {
    query.split('&').find_map(|entry| {
        let (key, value) = entry.split_once('=')?;
        if key != "file_id" {
            return None;
        }
        value.parse::<i64>().ok().filter(|id| *id > 0)
    })
}

fn store_nexus_mod_cache(
    conn: &Connection,
    link: &NexusModLink,
    body: &Value,
    fetched_at: &str,
) -> SlimResult<NexusModCache> {
    let data = body.get("data").unwrap_or(body);
    let name = data
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Unknown Nexus mod")
        .to_string();
    let version = data
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let updated_time = data
        .get("updated_time")
        .and_then(Value::as_str)
        .map(str::to_string);
    let endorsement_count = data.get("endorsement_count").and_then(Value::as_i64);
    let mod_downloads = data.get("mod_downloads").and_then(Value::as_i64);
    let raw_json = serde_json::to_string(body)?;

    conn.execute(
        "INSERT INTO nexus_mod_cache (
             mod_id, game_domain, nexus_mod_id, name, version, updated_time,
             endorsement_count, mod_downloads, fetched_at, raw_json
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(mod_id) DO UPDATE SET
             game_domain = excluded.game_domain,
             nexus_mod_id = excluded.nexus_mod_id,
             name = excluded.name,
             version = excluded.version,
             updated_time = excluded.updated_time,
             endorsement_count = excluded.endorsement_count,
             mod_downloads = excluded.mod_downloads,
             fetched_at = excluded.fetched_at,
             raw_json = excluded.raw_json",
        params![
            &link.mod_id,
            &link.game_domain,
            link.nexus_mod_id,
            &name,
            &version,
            updated_time,
            endorsement_count,
            mod_downloads,
            fetched_at,
            raw_json
        ],
    )?;

    get_nexus_mod_cache(conn, &link.mod_id)
}

fn store_nexus_file_cache(
    conn: &Connection,
    mod_id: &str,
    body: &Value,
    fetched_at: &str,
) -> SlimResult<usize> {
    conn.execute(
        "DELETE FROM nexus_file_cache WHERE mod_id = ?1",
        params![mod_id],
    )?;
    let files = body
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            body.get("data")
                .cloned()
                .map(|value| vec![value])
                .unwrap_or_default()
        });
    for file in &files {
        let update_group_version = file.get("update_group_version").unwrap_or(&Value::Null);
        conn.execute(
            "INSERT INTO nexus_file_cache (
                 id, mod_id, nexus_file_id, name, version, category_name, is_primary,
                 uploaded_time, mod_version, file_name, size_in_bytes, fetched_at, raw_json,
                 nexus_global_file_id
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                uuid::Uuid::new_v4().to_string(),
                mod_id,
                file.get("game_scoped_id")
                    .or_else(|| file.get("file_id"))
                    .and_then(as_i64_flexible)
                    .unwrap_or(0),
                file.get("name").and_then(Value::as_str).unwrap_or(""),
                file.get("version").and_then(Value::as_str).unwrap_or(""),
                file.pointer("/category/name")
                    .or_else(|| file.get("category_name"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                if file
                    .get("is_primary")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    1
                } else {
                    0
                },
                file.get("uploaded_at")
                    .or_else(|| file.get("uploaded_time"))
                    .and_then(Value::as_str),
                file.get("mod_version")
                    .or_else(|| update_group_version.get("position"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                file.get("file_name").and_then(Value::as_str).unwrap_or(""),
                file.get("size_in_bytes").and_then(Value::as_i64),
                fetched_at,
                serde_json::to_string(file)?,
                file.get("id").and_then(Value::as_str)
            ],
        )?;
    }
    Ok(files.len())
}

fn store_nexus_requirements(
    conn: &Connection,
    mod_id: &str,
    game_domain: &str,
    body: &Value,
    fetched_at: &str,
) -> SlimResult<usize> {
    conn.execute(
        "DELETE FROM nexus_requirements WHERE mod_id = ?1 AND source = 'nexus'",
        params![mod_id],
    )?;
    let requirements = extract_requirements(body);
    for requirement in &requirements {
        conn.execute(
            "INSERT INTO nexus_requirements (
                 id, mod_id, required_game_domain, required_nexus_mod_id, required_name,
                 requirement_type, source, notes, fetched_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'nexus', ?7, ?8)",
            params![
                uuid::Uuid::new_v4().to_string(),
                mod_id,
                requirement
                    .required_game_domain
                    .clone()
                    .unwrap_or_else(|| game_domain.to_string()),
                requirement.required_nexus_mod_id,
                &requirement.required_name,
                &requirement.requirement_type,
                &requirement.notes,
                fetched_at
            ],
        )?;
    }
    Ok(requirements.len())
}

fn get_nexus_mod_cache(conn: &Connection, mod_id: &str) -> SlimResult<NexusModCache> {
    Ok(conn.query_row(
        "SELECT mod_id, game_domain, nexus_mod_id, name, version, updated_time,
                endorsement_count, mod_downloads, fetched_at
         FROM nexus_mod_cache
         WHERE mod_id = ?1",
        params![mod_id],
        row_to_nexus_mod_cache,
    )?)
}

fn row_to_nexus_mod_cache(row: &rusqlite::Row<'_>) -> rusqlite::Result<NexusModCache> {
    Ok(NexusModCache {
        mod_id: row.get(0)?,
        game_domain: row.get(1)?,
        nexus_mod_id: row.get(2)?,
        name: row.get(3)?,
        version: row.get(4)?,
        updated_time: row.get(5)?,
        endorsement_count: row.get(6)?,
        mod_downloads: row.get(7)?,
        fetched_at: row.get(8)?,
    })
}

#[derive(Debug, Clone)]
struct ExtractedRequirement {
    required_game_domain: Option<String>,
    required_nexus_mod_id: Option<i64>,
    required_name: String,
    requirement_type: String,
    notes: String,
}

fn extract_requirements(body: &Value) -> Vec<ExtractedRequirement> {
    if let Some(dependencies) = body.get("dependencies").and_then(Value::as_array) {
        return dependencies
            .iter()
            .flat_map(|dependency| {
                dependency
                    .get("candidate_groups")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|group| {
                        let mod_value = group.get("mod").unwrap_or(&Value::Null);
                        let name = mod_value.get("name").and_then(Value::as_str).unwrap_or("");
                        if name.trim().is_empty() {
                            return None;
                        }
                        Some(ExtractedRequirement {
                            required_game_domain: mod_value
                                .pointer("/game/domain_name")
                                .or_else(|| mod_value.pointer("/game/domain"))
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            required_nexus_mod_id: mod_value
                                .get("game_scoped_id")
                                .and_then(as_i64_flexible),
                            required_name: name.to_string(),
                            requirement_type: "file_dependency".into(),
                            notes: serde_json::to_string(group).unwrap_or_default(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
    }
    let Some(values) = body.get("requirements").and_then(Value::as_array) else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(|value| {
            let required_name = value
                .get("name")
                .or_else(|| value.get("required_name"))
                .or_else(|| value.get("mod_name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let required_nexus_mod_id = value
                .get("mod_id")
                .or_else(|| value.get("required_nexus_mod_id"))
                .and_then(Value::as_i64);
            if required_name.is_empty() && required_nexus_mod_id.is_none() {
                return None;
            }
            Some(ExtractedRequirement {
                required_game_domain: value
                    .get("domain_name")
                    .or_else(|| value.get("game_domain"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                required_nexus_mod_id,
                required_name: if required_name.is_empty() {
                    format!("Nexus mod {required_nexus_mod_id:?}")
                } else {
                    required_name
                },
                requirement_type: value
                    .get("type")
                    .or_else(|| value.get("requirement_type"))
                    .and_then(Value::as_str)
                    .unwrap_or("required")
                    .to_string(),
                notes: serde_json::to_string(value).unwrap_or_default(),
            })
        })
        .collect()
}

fn as_i64_flexible(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

struct NexusResponse {
    body: Value,
    hourly_remaining: Option<i64>,
    daily_remaining: Option<i64>,
}

fn nexus_get(api_key: &str, path: &str) -> SlimResult<NexusResponse> {
    nexus_get_from(NEXUS_API_BASE, api_key, path)
}

fn nexus_v1_get(api_key: &str, path: &str) -> SlimResult<NexusResponse> {
    nexus_get_from(NEXUS_LEGACY_API_BASE, api_key, path)
}

fn nexus_get_from(base: &str, api_key: &str, path: &str) -> SlimResult<NexusResponse> {
    let url = format!("{base}{path}");
    let response = reqwest::blocking::Client::new()
        .get(&url)
        .header("apikey", api_key)
        .header("Application-Name", "SLiM-CC")
        .header("Application-Version", env!("CARGO_PKG_VERSION"))
        .header("Protocol-Version", "1.0.0")
        .header(
            "User-Agent",
            format!("SLiM-CC/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()?;
    let status = response.status();
    let hourly_remaining = header_i64(response.headers(), "x-rl-hourly-remaining");
    let daily_remaining = header_i64(response.headers(), "x-rl-daily-remaining");
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(SlimError::Process(format!(
            "Nexus API request failed ({status}): {}",
            body.trim()
        )));
    }
    Ok(NexusResponse {
        body: response.json()?,
        hourly_remaining,
        daily_remaining,
    })
}

fn header_i64(headers: &reqwest::header::HeaderMap, name: &str) -> Option<i64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

fn url_path(value: &str) -> String {
    value.trim().replace(' ', "%20")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_nexus_mod_link_stores_optional_source_mapping() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/005_nexus_metadata.sql"))
            .expect("create nexus schema");
        conn.execute_batch(include_str!("../migrations/008_nexus_api_cache.sql"))
            .expect("create nexus cache schema");
        conn.execute_batch(include_str!("../migrations/009_nexus_v3_file_mapping.sql"))
            .expect("create nexus v3 schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'SkyUI', '/mods/skyui', '/mods/skyui', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mod");

        let link = upsert_nexus_mod_link(
            &conn,
            UpdateNexusModLinkRequest {
                mod_id: "mod-a".into(),
                game_domain: "SkyrimSpecialEdition".into(),
                nexus_mod_id: Some(12604),
                nexus_file_id: Some(35169),
                source_url: "https://www.nexusmods.com/skyrimspecialedition/mods/12604".into(),
            },
        )
        .expect("upsert link")
        .expect("stored link");

        assert_eq!(link.game_domain, "skyrimspecialedition");
        assert_eq!(link.nexus_mod_id, 12604);
        assert_eq!(link.nexus_file_id, Some(35169));
        assert_eq!(list_nexus_mod_links(&conn, "instance-a").unwrap().len(), 1);
    }

    #[test]
    fn parse_api_key_accepts_markdown_or_plain_key_without_logging_it() {
        assert_eq!(
            crate::settings::parse_api_key("abc123").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            crate::settings::parse_api_key("api_key = `abc/123==`").as_deref(),
            Some("abc/123==")
        );
        assert!(crate::settings::parse_api_key("# comment\n\n").is_none());
    }

    #[test]
    fn extract_requirements_reads_optional_api_requirement_shapes() {
        let body = serde_json::json!({
            "requirements": [
                {
                    "name": "Address Library",
                    "mod_id": 32444,
                    "domain_name": "skyrimspecialedition",
                    "type": "required"
                }
            ]
        });

        let requirements = extract_requirements(&body);

        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].required_name, "Address Library");
        assert_eq!(requirements[0].required_nexus_mod_id, Some(32444));
    }

    #[test]
    fn parse_nexus_source_link_reads_nxm_mod_manager_url_without_secret_query() {
        let parsed = parse_nexus_source_link(
            "nxm://skyrimspecialedition/mods/179647/files/750608?key=secret&expires=1779074488&user_id=9598179",
        )
        .expect("parse nxm link");

        assert_eq!(parsed.game_domain, "skyrimspecialedition");
        assert_eq!(parsed.nexus_mod_id, 179647);
        assert_eq!(parsed.nexus_file_id, Some(750608));
        assert_eq!(
            parsed.sanitized_url,
            "nxm://skyrimspecialedition/mods/179647/files/750608"
        );
    }

    #[test]
    fn parse_nexus_source_link_reads_regular_nexus_url_with_file_id() {
        let parsed = parse_nexus_source_link(
            "https://www.nexusmods.com/skyrimspecialedition/mods/179647?tab=files&file_id=750608",
        )
        .expect("parse nexus page link");

        assert_eq!(parsed.game_domain, "skyrimspecialedition");
        assert_eq!(parsed.nexus_mod_id, 179647);
        assert_eq!(parsed.nexus_file_id, Some(750608));
        assert_eq!(
            parsed.sanitized_url,
            "https://www.nexusmods.com/skyrimspecialedition/mods/179647?file_id=750608"
        );
    }

    #[test]
    fn parse_nexus_source_link_ignores_cdn_download_url_without_file_id() {
        let parsed = parse_nexus_source_link(
            "https://supporter-files.nexus-cdn.com/1704/179647/IDE%20Hearthfire-179647-1-1778405504.zip?md5=secret&expires=1778916199&user_id=9598179",
        );

        assert!(parsed.is_none());
    }

    #[test]
    fn direct_cdn_validation_accepts_only_official_https_hosts() {
        assert!(validated_nexus_cdn_url(
            "https://supporter-files.nexus-cdn.com/path/mod.zip?token=secret"
        )
        .is_some());
        assert!(validated_nexus_cdn_url("http://supporter-files.nexus-cdn.com/mod.zip").is_none());
        assert!(
            validated_nexus_cdn_url("https://nexus-cdn.com.attacker.example/mod.zip").is_none()
        );
        assert!(validated_nexus_cdn_url("https://example.com/mod.zip").is_none());
    }

    #[test]
    fn content_disposition_names_are_reduced_to_safe_file_names() {
        let name = content_disposition_file_name("attachment; filename=\"Example Mod-1-2-3.zip\"")
            .and_then(|value| safe_download_file_name(&value));
        assert_eq!(name.as_deref(), Some("Example Mod-1-2-3.zip"));
        assert_eq!(
            safe_download_file_name("../../outside.zip").as_deref(),
            Some("outside.zip")
        );
        assert!(safe_download_file_name("..").is_none());
    }

    #[test]
    #[ignore = "requires SLIMCC_LIVE_NEXUS_URL and optionally SLIMCC_LIVE_NEXUS_API_KEY"]
    fn live_nexus_download_uses_the_production_backend() {
        let source_url = std::env::var("SLIMCC_LIVE_NEXUS_URL").expect("live Nexus URL");
        let api_key = std::env::var("SLIMCC_LIVE_NEXUS_API_KEY").ok();
        let download_root = std::env::temp_dir().join(format!(
            "slimcc-live-nexus-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&download_root).expect("create isolated download folder");
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            "CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .expect("create settings schema");
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES ('mod_download_path', ?1)",
            [download_root.to_string_lossy().as_ref()],
        )
        .expect("configure download path");
        if let Some(api_key) = api_key {
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES ('nexus_api_key', ?1)",
                [api_key],
            )
            .expect("configure Nexus API key");
        }

        let result = download_nexus_file(&conn, crate::models::NexusDownloadRequest { source_url })
            .expect("download through production backend");
        assert!(result.bytes_written > 0);
        assert!(result.path.is_file());
        assert!(!result.file_name.contains('/'));
        let _ = std::fs::remove_dir_all(download_root);
    }

    #[test]
    fn nexus_requirement_status_matches_installed_mod_by_nexus_id() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/005_nexus_metadata.sql"))
            .expect("create nexus schema");
        conn.execute_batch(include_str!("../migrations/008_nexus_api_cache.sql"))
            .expect("create nexus cache schema");
        conn.execute_batch(include_str!("../migrations/009_nexus_v3_file_mapping.sql"))
            .expect("create nexus v3 schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES
                ('mod-a', 'instance-a', 'Needs Address Library', '/mods/a', '/mods/a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('mod-b', 'instance-a', 'Address Library', '/mods/b', '/mods/b', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mods");
        conn.execute(
            "INSERT INTO nexus_mod_links (mod_id, game_domain, nexus_mod_id, nexus_file_id, source_url, created_at, updated_at)
             VALUES ('mod-b', 'skyrimspecialedition', 32444, NULL, '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert nexus link");
        conn.execute(
            "INSERT INTO nexus_requirements (
                id, mod_id, required_game_domain, required_nexus_mod_id, required_name,
                requirement_type, source, notes, fetched_at
             )
             VALUES (
                'req-a', 'mod-a', 'skyrimspecialedition', 32444, 'Address Library',
                'required', 'nexus', '', '2026-01-01T00:00:00Z'
             )",
            [],
        )
        .expect("insert nexus requirement");

        let requirements = list_nexus_requirement_status(&conn, "mod-a").expect("status");

        assert_eq!(requirements.len(), 1);
        assert!(requirements[0].satisfied);
        assert_eq!(requirements[0].matched_mod_id.as_deref(), Some("mod-b"));
        assert_eq!(requirements[0].status, "satisfied");
    }
}
