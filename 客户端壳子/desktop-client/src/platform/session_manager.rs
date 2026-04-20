use crate::platform::managed_home::{resolve_user_codex_home, RuntimeSessionMetadata};
use rusqlite::{types::Value, Connection, OpenFlags, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const OFFICIAL_HOME_ID: &str = "__official__";
const STATE_DB_FILE: &str = "state_5.sqlite";
const SESSION_INDEX_FILE: &str = "session_index.jsonl";
const SESSION_DIRS: [&str; 2] = ["sessions", "archived_sessions"];
const TOKEN_STATS_READ_CHUNK_BYTES: usize = 64 * 1024;
const SESSION_TRASH_ROOT_DIR: &str = "codex-session-trash";
const GLOBAL_STATE_FILE: &str = ".codex-global-state.json";
const BACKUP_FILE_NAMES: [&str; 3] = [STATE_DB_FILE, SESSION_INDEX_FILE, GLOBAL_STATE_FILE];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionHome {
    pub id: String,
    pub label: String,
    pub data_dir: String,
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionLocation {
    pub home_id: String,
    pub home_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub session_id: String,
    pub title: String,
    pub cwd: String,
    pub updated_at: Option<i64>,
    pub location_count: usize,
    pub locations: Vec<SessionLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionGroup {
    pub cwd: String,
    pub latest_updated_at: Option<i64>,
    pub sessions: Vec<SessionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTokenStats {
    pub session_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncItem {
    pub home_id: String,
    pub home_label: String,
    pub added_session_count: usize,
    pub backup_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncSummary {
    pub home_count: usize,
    pub session_universe_count: usize,
    pub mutated_home_count: usize,
    pub total_synced_session_count: usize,
    pub items: Vec<SessionSyncItem>,
    pub backup_dirs: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionVisibilityRepairItem {
    pub home_id: String,
    pub home_label: String,
    pub target_provider: String,
    pub changed_rollout_file_count: usize,
    pub updated_sqlite_row_count: usize,
    pub backup_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionVisibilityRepairSummary {
    pub home_count: usize,
    pub mutated_home_count: usize,
    pub changed_rollout_file_count: usize,
    pub updated_sqlite_row_count: usize,
    pub items: Vec<SessionVisibilityRepairItem>,
    pub backup_dirs: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrashedSessionLocation {
    pub home_id: String,
    pub home_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrashedSessionRecord {
    pub session_id: String,
    pub title: String,
    pub cwd: String,
    pub deleted_at: Option<i64>,
    pub location_count: usize,
    pub locations: Vec<TrashedSessionLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTrashSummary {
    pub requested_session_count: usize,
    pub trashed_session_count: usize,
    pub trashed_home_count: usize,
    pub trash_dirs: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRestoreSummary {
    pub requested_session_count: usize,
    pub restored_session_count: usize,
    pub restored_home_count: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
struct SessionHomeEntry {
    id: String,
    label: String,
    data_dir: PathBuf,
    managed: bool,
}

#[derive(Debug, Clone)]
struct ThreadRowData {
    columns: Vec<String>,
    values: Vec<Value>,
}

impl ThreadRowData {
    fn get_value(&self, column: &str) -> Option<&Value> {
        self.columns
            .iter()
            .position(|item| item == column)
            .and_then(|index| self.values.get(index))
    }

    fn get_text(&self, column: &str) -> Option<String> {
        match self.get_value(column)? {
            Value::Text(value) => Some(value.clone()),
            Value::Integer(value) => Some(value.to_string()),
            Value::Real(value) => Some(value.to_string()),
            _ => None,
        }
    }

    fn get_i64(&self, column: &str) -> Option<i64> {
        match self.get_value(column)? {
            Value::Integer(value) => Some(*value),
            Value::Text(value) => value.parse::<i64>().ok(),
            _ => None,
        }
    }

    fn set_text(&mut self, column: &str, value: String) {
        if let Some(index) = self.columns.iter().position(|item| item == column) {
            if let Some(slot) = self.values.get_mut(index) {
                *slot = Value::Text(value);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ThreadSnapshot {
    id: String,
    title: String,
    cwd: String,
    updated_at: Option<i64>,
    rollout_path: PathBuf,
    row_data: ThreadRowData,
    session_index_entry: JsonValue,
    source_root: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashedSessionManifest {
    session_id: String,
    title: String,
    cwd: String,
    home_id: String,
    home_label: String,
    home_root: PathBuf,
    original_rollout_path: PathBuf,
    relative_rollout_path: String,
    session_index_entry: JsonValue,
    thread_row: JsonValue,
    deleted_at: Option<String>,
}

#[derive(Debug, Clone)]
struct TrashedSessionEntry {
    entry_dir: PathBuf,
    manifest: TrashedSessionManifest,
    trashed_rollout_path: PathBuf,
}

pub fn list_session_homes(app_root: &Path) -> Result<Vec<SessionHome>, String> {
    Ok(collect_session_homes(app_root)?
        .into_iter()
        .map(|home| SessionHome {
            id: home.id,
            label: home.label,
            data_dir: home.data_dir.to_string_lossy().to_string(),
            managed: home.managed,
        })
        .collect())
}

pub fn list_session_groups(app_root: &Path) -> Result<Vec<SessionGroup>, String> {
    let homes = collect_session_homes(app_root)?;
    let mut session_map = HashMap::<String, SessionRecord>::new();

    for home in &homes {
        for snapshot in load_thread_snapshots(home)? {
            let entry = session_map
                .entry(snapshot.id.clone())
                .or_insert_with(|| SessionRecord {
                    session_id: snapshot.id.clone(),
                    title: snapshot.title.clone(),
                    cwd: snapshot.cwd.clone(),
                    updated_at: snapshot.updated_at,
                    location_count: 0,
                    locations: Vec::new(),
                });

            if entry.updated_at.unwrap_or_default() < snapshot.updated_at.unwrap_or_default() {
                entry.updated_at = snapshot.updated_at;
            }
            if entry.title.trim().is_empty() {
                entry.title = snapshot.title.clone();
            }
            if entry.cwd.trim().is_empty() {
                entry.cwd = snapshot.cwd.clone();
            }

            entry.locations.push(SessionLocation {
                home_id: home.id.clone(),
                home_label: home.label.clone(),
            });
            entry.location_count = entry.locations.len();
        }
    }

    let mut groups = HashMap::<String, Vec<SessionRecord>>::new();
    for session in session_map.into_values() {
        groups.entry(session.cwd.clone()).or_default().push(session);
    }

    let mut result = groups
        .into_iter()
        .map(|(cwd, mut sessions)| {
            sessions.sort_by(|left, right| {
                right
                    .updated_at
                    .unwrap_or_default()
                    .cmp(&left.updated_at.unwrap_or_default())
                    .then_with(|| left.title.cmp(&right.title))
            });
            SessionGroup {
                latest_updated_at: sessions.first().and_then(|session| session.updated_at),
                cwd,
                sessions,
            }
        })
        .collect::<Vec<_>>();

    result.sort_by(|left, right| {
        right
            .latest_updated_at
            .unwrap_or_default()
            .cmp(&left.latest_updated_at.unwrap_or_default())
            .then_with(|| left.cwd.cmp(&right.cwd))
    });

    Ok(result)
}

pub fn get_session_token_stats(app_root: &Path, session_ids: &[String]) -> Result<Vec<SessionTokenStats>, String> {
    let requested_ids = session_ids
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<HashSet<_>>();
    if requested_ids.is_empty() {
        return Ok(Vec::new());
    }

    let homes = collect_session_homes(app_root)?;
    let mut pending_ids = requested_ids.clone();
    let mut stats_by_id = HashMap::<String, SessionTokenStats>::new();

    for home in &homes {
        if pending_ids.is_empty() {
            break;
        }
        for snapshot in load_thread_snapshots(home)? {
            if !pending_ids.contains(&snapshot.id) {
                continue;
            }
            if let Some((input_tokens, output_tokens, total_tokens)) =
                read_token_stats_from_rollout(&snapshot.rollout_path)
            {
                stats_by_id.insert(
                    snapshot.id.clone(),
                    SessionTokenStats {
                        session_id: snapshot.id.clone(),
                        input_tokens,
                        output_tokens,
                        total_tokens,
                    },
                );
                pending_ids.remove(&snapshot.id);
            }
        }
    }

    let mut stats = stats_by_id.into_values().collect::<Vec<_>>();
    stats.sort_by(|left, right| left.session_id.cmp(&right.session_id));
    Ok(stats)
}

pub fn sync_sessions_across_homes(app_root: &Path) -> Result<SessionSyncSummary, String> {
    let homes = collect_session_homes(app_root)?;
    if homes.len() < 2 {
        return Err("至少需要两个会话目录才能同步。".to_string());
    }

    let mut thread_universe = HashMap::<String, ThreadSnapshot>::new();
    let mut existing_ids_by_home = HashMap::<String, HashSet<String>>::new();

    for home in &homes {
        let snapshots = load_thread_snapshots(home)?;
        let ids = snapshots
            .iter()
            .map(|item| item.id.clone())
            .collect::<HashSet<_>>();
        for snapshot in snapshots {
            thread_universe
                .entry(snapshot.id.clone())
                .or_insert(snapshot);
        }
        existing_ids_by_home.insert(home.id.clone(), ids);
    }

    let mut universe_ids = thread_universe.keys().cloned().collect::<Vec<_>>();
    universe_ids.sort();

    let mut items = Vec::new();
    let mut backup_dirs = Vec::new();
    let mut mutated_home_count = 0usize;
    let mut total_synced_session_count = 0usize;

    for home in &homes {
        let existing_ids = existing_ids_by_home
            .get(&home.id)
            .cloned()
            .unwrap_or_default();
        let missing_snapshots = universe_ids
            .iter()
            .filter(|id| !existing_ids.contains(*id))
            .filter_map(|id| thread_universe.get(id).cloned())
            .collect::<Vec<_>>();

        if missing_snapshots.is_empty() {
            items.push(SessionSyncItem {
                home_id: home.id.clone(),
                home_label: home.label.clone(),
                added_session_count: 0,
                backup_dir: None,
            });
            continue;
        }

        let backup_dir = sync_missing_threads_to_home(home, &missing_snapshots)?;
        let backup_dir_string = backup_dir.to_string_lossy().to_string();
        backup_dirs.push(backup_dir_string.clone());
        mutated_home_count += 1;
        total_synced_session_count += missing_snapshots.len();

        items.push(SessionSyncItem {
            home_id: home.id.clone(),
            home_label: home.label.clone(),
            added_session_count: missing_snapshots.len(),
            backup_dir: Some(backup_dir_string),
        });
    }

    let message = if total_synced_session_count == 0 {
        "所有会话目录已经同步，无需补齐。".to_string()
    } else {
        format!(
            "已为 {} 个目录补齐 {} 条会话。",
            mutated_home_count, total_synced_session_count
        )
    };

    Ok(SessionSyncSummary {
        home_count: homes.len(),
        session_universe_count: thread_universe.len(),
        mutated_home_count,
        total_synced_session_count,
        items,
        backup_dirs,
        message,
    })
}

pub fn repair_session_visibility(app_root: &Path) -> Result<SessionVisibilityRepairSummary, String> {
    let homes = collect_session_homes(app_root)?;
    let mut items = Vec::new();
    let mut backup_dirs = Vec::new();
    let mut mutated_home_count = 0usize;
    let mut changed_rollout_file_count = 0usize;
    let mut updated_sqlite_row_count = 0usize;

    for home in &homes {
        let target_provider = read_target_provider(&home.data_dir)?;
        let rollout_changes = collect_rollout_provider_changes(&home.data_dir, &target_provider)?;
        let sqlite_rows_to_update = count_sqlite_rows_to_update(&home.data_dir, &target_provider)?;

        if rollout_changes.is_empty() && sqlite_rows_to_update == 0 {
            items.push(SessionVisibilityRepairItem {
                home_id: home.id.clone(),
                home_label: home.label.clone(),
                target_provider,
                changed_rollout_file_count: 0,
                updated_sqlite_row_count: 0,
                backup_dir: None,
            });
            continue;
        }

        let backup_dir = backup_visibility_files(&home.data_dir, &rollout_changes, sqlite_rows_to_update > 0)?;
        let backup_dir_string = backup_dir.to_string_lossy().to_string();
        let repaired = repair_single_home_visibility(&home.data_dir, &target_provider, &rollout_changes);
        let sqlite_rows_updated = match repaired {
            Ok(value) => value,
            Err(error) => {
                let restore_result = restore_visibility_files_from_backup(
                    &home.data_dir,
                    &backup_dir,
                    sqlite_rows_to_update > 0,
                );
                if let Err(restore_error) = restore_result {
                    return Err(format!(
                        "修复目录 {} 失败：{}；自动回滚也失败：{}",
                        home.label, error, restore_error
                    ));
                }
                return Err(format!("修复目录 {} 失败：{}；已自动回滚。", home.label, error));
            }
        };

        mutated_home_count += 1;
        changed_rollout_file_count += rollout_changes.len();
        updated_sqlite_row_count += sqlite_rows_updated;
        backup_dirs.push(backup_dir_string.clone());
        items.push(SessionVisibilityRepairItem {
            home_id: home.id.clone(),
            home_label: home.label.clone(),
            target_provider,
            changed_rollout_file_count: rollout_changes.len(),
            updated_sqlite_row_count: sqlite_rows_updated,
            backup_dir: Some(backup_dir_string),
        });
    }

    let message = if mutated_home_count == 0 {
        "所有会话目录的 provider 元数据已经一致，无需修复。".to_string()
    } else {
        format!(
            "已为 {} 个目录修复会话可见性：改写 {} 个 rollout 文件，更新 {} 条 SQLite 记录。",
            mutated_home_count, changed_rollout_file_count, updated_sqlite_row_count
        )
    };

    Ok(SessionVisibilityRepairSummary {
        home_count: homes.len(),
        mutated_home_count,
        changed_rollout_file_count,
        updated_sqlite_row_count,
        items,
        backup_dirs,
        message,
    })
}

pub fn move_sessions_to_trash(app_root: &Path, session_ids: &[String]) -> Result<SessionTrashSummary, String> {
    let requested_ids = session_ids
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<HashSet<_>>();
    if requested_ids.is_empty() {
        return Err("请至少选择一条会话。".to_string());
    }

    let homes = collect_session_homes(app_root)?;
    let trash_root = create_trash_root_dir(app_root)?;
    let mut trashed_session_ids = HashSet::new();
    let mut trashed_home_count = 0usize;

    for home in &homes {
        let snapshots = load_thread_snapshots(home)?
            .into_iter()
            .filter(|snapshot| requested_ids.contains(&snapshot.id))
            .collect::<Vec<_>>();
        if snapshots.is_empty() {
            continue;
        }

        move_threads_to_trash(home, &trash_root, &snapshots)?;
        trashed_home_count += 1;
        for snapshot in snapshots {
            trashed_session_ids.insert(snapshot.id);
        }
    }

    if trashed_home_count == 0 {
        return Ok(SessionTrashSummary {
            requested_session_count: requested_ids.len(),
            trashed_session_count: 0,
            trashed_home_count: 0,
            trash_dirs: Vec::new(),
            message: "所选会话在当前目录集合中不存在，无需处理。".to_string(),
        });
    }

    Ok(SessionTrashSummary {
        requested_session_count: requested_ids.len(),
        trashed_session_count: trashed_session_ids.len(),
        trashed_home_count,
        trash_dirs: vec![trash_root.to_string_lossy().to_string()],
        message: format!("已将 {} 条会话移到废纸篓。", trashed_session_ids.len()),
    })
}

pub fn list_trashed_sessions(app_root: &Path) -> Result<Vec<TrashedSessionRecord>, String> {
    let entries = load_trash_entries(app_root)?;
    let mut session_map = HashMap::<String, TrashedSessionRecord>::new();

    for entry in entries {
        let deleted_at = parse_deleted_at(entry.manifest.deleted_at.as_deref());
        let record = session_map
            .entry(entry.manifest.session_id.clone())
            .or_insert_with(|| TrashedSessionRecord {
                session_id: entry.manifest.session_id.clone(),
                title: entry.manifest.title.clone(),
                cwd: entry.manifest.cwd.clone(),
                deleted_at,
                location_count: 0,
                locations: Vec::new(),
            });

        if deleted_at.unwrap_or_default() > record.deleted_at.unwrap_or_default() {
            record.deleted_at = deleted_at;
        }
        if record.title.trim().is_empty() {
            record.title = entry.manifest.title.clone();
        }
        if record.cwd.trim().is_empty() {
            record.cwd = entry.manifest.cwd.clone();
        }
        record.locations.push(TrashedSessionLocation {
            home_id: entry.manifest.home_id.clone(),
            home_label: entry.manifest.home_label.clone(),
        });
        record.location_count = record.locations.len();
    }

    let mut sessions = session_map.into_values().collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        right
            .deleted_at
            .unwrap_or_default()
            .cmp(&left.deleted_at.unwrap_or_default())
            .then_with(|| left.cwd.cmp(&right.cwd))
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(sessions)
}

pub fn restore_sessions_from_trash(app_root: &Path, session_ids: &[String]) -> Result<SessionRestoreSummary, String> {
    let requested_ids = session_ids
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<HashSet<_>>();
    if requested_ids.is_empty() {
        return Err("请至少选择一条待恢复会话。".to_string());
    }

    let entries = load_trash_entries(app_root)?
        .into_iter()
        .filter(|entry| requested_ids.contains(&entry.manifest.session_id))
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return Ok(SessionRestoreSummary {
            requested_session_count: requested_ids.len(),
            restored_session_count: 0,
            restored_home_count: 0,
            message: "所选会话在废纸篓中不存在，无需恢复。".to_string(),
        });
    }

    let mut restored_session_ids = HashSet::new();
    let mut restored_home_ids = HashSet::new();

    for entry in &entries {
        restore_trashed_session_entry(entry)?;
        restored_session_ids.insert(entry.manifest.session_id.clone());
        restored_home_ids.insert(entry.manifest.home_id.clone());
    }

    Ok(SessionRestoreSummary {
        requested_session_count: requested_ids.len(),
        restored_session_count: restored_session_ids.len(),
        restored_home_count: restored_home_ids.len(),
        message: format!("已恢复 {} 条会话。", restored_session_ids.len()),
    })
}

fn collect_session_homes(app_root: &Path) -> Result<Vec<SessionHomeEntry>, String> {
    let mut homes = Vec::new();
    let mut seen = HashSet::new();

    let official_home = resolve_user_codex_home()
        .map_err(|error| format!("解析官方 Codex 目录失败：{error}"))?;
    let official_key = official_home.to_string_lossy().to_string();
    if seen.insert(official_key) {
        homes.push(SessionHomeEntry {
            id: OFFICIAL_HOME_ID.to_string(),
            label: "官方目录".to_string(),
            data_dir: official_home,
            managed: false,
        });
    }

    let runtime_root = app_root.join("runtime");
    if runtime_root.exists() {
        let entries = fs::read_dir(&runtime_root)
            .map_err(|error| format!("读取运行时目录失败 ({}): {}", runtime_root.display(), error))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!("读取运行时目录项失败 ({}): {}", runtime_root.display(), error)
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let metadata_path = path.join("runtime-session.json");
            if !metadata_path.exists() {
                continue;
            }
            let metadata = fs::read(&metadata_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<RuntimeSessionMetadata>(&bytes).ok());
            let Some(metadata) = metadata else {
                continue;
            };
            let codex_home = path.join(&metadata.profile_key);
            let key = codex_home.to_string_lossy().to_string();
            if !seen.insert(key) {
                continue;
            }
            let short_session = metadata
                .session_id
                .chars()
                .take(8)
                .collect::<String>();
            homes.push(SessionHomeEntry {
                id: metadata.session_id.clone(),
                label: format!("平台代理 · {}", short_session),
                data_dir: codex_home,
                managed: true,
            });
        }
    }

    Ok(homes)
}

fn load_thread_snapshots(home: &SessionHomeEntry) -> Result<Vec<ThreadSnapshot>, String> {
    let db_path = home.data_dir.join(STATE_DB_FILE);
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let connection = open_readonly_connection(&db_path)?;
    let columns = read_thread_columns(&connection)?;
    let select_columns = columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!("SELECT {} FROM threads", select_columns);
    let mut statement = connection
        .prepare(&query)
        .map_err(|error| format!("读取会话线程失败 ({}): {}", home.label, error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| format!("查询会话线程失败 ({}): {}", home.label, error))?;
    let session_index_map = read_session_index_map(&home.data_dir)?;

    let mut snapshots = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("迭代会话线程失败 ({}): {}", home.label, error))?
    {
        let mut values = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            values.push(
                row.get::<usize, Value>(index)
                    .map_err(|error| format!("解析线程记录失败 ({}): {}", home.label, error))?,
            );
        }

        let row_data = ThreadRowData {
            columns: columns.clone(),
            values,
        };
        let id = row_data
            .get_text("id")
            .ok_or_else(|| format!("线程缺少 id 字段 ({})", home.label))?;
        let rollout_path = row_data
            .get_text("rollout_path")
            .ok_or_else(|| format!("线程 {} 缺少 rollout_path ({})", id, home.label))?;
        let title = row_data
            .get_text("title")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| id.clone());
        let cwd = row_data
            .get_text("cwd")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "未知工作区".to_string());
        let updated_at = row_data.get_i64("updated_at");
        let session_index_entry = session_index_map.get(&id).cloned().unwrap_or_else(|| {
            build_fallback_session_index_entry(&id, &title, updated_at)
        });

        snapshots.push(ThreadSnapshot {
            id,
            title,
            cwd,
            updated_at,
            rollout_path: PathBuf::from(rollout_path),
            row_data,
            session_index_entry,
            source_root: home.data_dir.clone(),
        });
    }

    Ok(snapshots)
}

fn read_token_stats_from_rollout(rollout_path: &Path) -> Option<(u64, u64, u64)> {
    let mut file = File::open(rollout_path).ok()?;
    let file_len = file.metadata().ok()?.len();
    let mut offset = file_len;
    let mut pending_prefix = Vec::new();

    while offset > 0 {
        let chunk_len = TOKEN_STATS_READ_CHUNK_BYTES.min(offset as usize);
        offset -= chunk_len as u64;
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut chunk = vec![0u8; chunk_len];
        file.read_exact(&mut chunk).ok()?;

        let starts_on_line_boundary = offset == 0 || byte_before_is_newline(&mut file, offset).ok()?;
        chunk.extend_from_slice(&pending_prefix);

        let parse_from_index = if starts_on_line_boundary {
            pending_prefix.clear();
            0
        } else if let Some(newline_index) = chunk.iter().position(|byte| *byte == b'\n') {
            pending_prefix = chunk[..newline_index].to_vec();
            newline_index + 1
        } else {
            pending_prefix = chunk;
            continue;
        };

        if let Some(stats) = parse_token_stats_lines(&chunk[parse_from_index..]) {
            return Some(stats);
        }
    }

    if pending_prefix.is_empty() {
        None
    } else {
        parse_token_stats_lines(&pending_prefix)
    }
}

fn byte_before_is_newline(file: &mut File, offset: u64) -> std::io::Result<bool> {
    if offset == 0 {
        return Ok(true);
    }
    file.seek(SeekFrom::Start(offset - 1))?;
    let mut byte = [0u8; 1];
    file.read_exact(&mut byte)?;
    Ok(byte[0] == b'\n')
}

fn parse_token_stats_lines(content: &[u8]) -> Option<(u64, u64, u64)> {
    for line in content.split(|byte| *byte == b'\n').rev() {
        let raw = String::from_utf8_lossy(line);
        let trimmed = raw.trim();
        if trimmed.is_empty()
            || !trimmed.contains("\"token_count\"")
            || !trimmed.contains("\"total_token_usage\"")
        {
            continue;
        }

        let parsed = serde_json::from_str::<JsonValue>(trimmed).ok()?;
        if parsed.get("type").and_then(|value| value.as_str()) != Some("event_msg") {
            continue;
        }
        let payload = parsed.get("payload")?;
        if payload.get("type").and_then(|value| value.as_str()) != Some("token_count") {
            continue;
        }
        let usage = payload
            .get("info")
            .and_then(|info| info.get("total_token_usage"))?;
        let input = usage
            .get("input_tokens")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let output = usage
            .get("output_tokens")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let total = usage
            .get("total_tokens")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        return Some((input, output, total));
    }
    None
}

fn sync_missing_threads_to_home(home: &SessionHomeEntry, snapshots: &[ThreadSnapshot]) -> Result<PathBuf, String> {
    let backup_dir = backup_sync_files(&home.data_dir)?;
    let index_map = read_session_index_map(&home.data_dir)?;
    let existing_index_ids = index_map.keys().cloned().collect::<HashSet<_>>();
    let db_path = home.data_dir.join(STATE_DB_FILE);
    let mut connection = Connection::open(&db_path)
        .map_err(|error| format!("打开目标数据库失败 ({}): {}", home.label, error))?;
    let target_columns = read_thread_columns(&connection)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("开启目标事务失败 ({}): {}", home.label, error))?;

    for snapshot in snapshots {
        let target_rollout_path = copy_rollout_file(snapshot, &home.data_dir)?;
        let mut row_data = snapshot.row_data.clone();
        row_data.set_text(
            "rollout_path",
            target_rollout_path.to_string_lossy().to_string(),
        );
        insert_thread_row(&transaction, &target_columns, &row_data)?;
    }

    transaction
        .commit()
        .map_err(|error| format!("提交目标事务失败 ({}): {}", home.label, error))?;

    append_session_index_entries(&home.data_dir, &existing_index_ids, snapshots)?;
    update_global_state(
        &home.data_dir,
        snapshots.iter().map(|snapshot| snapshot.cwd.as_str()),
    )?;

    Ok(backup_dir)
}

fn repair_single_home_visibility(
    data_dir: &Path,
    target_provider: &str,
    rollout_changes: &[RolloutProviderChange],
) -> Result<usize, String> {
    let updated_rows = update_sqlite_provider(data_dir, target_provider)?;
    for change in rollout_changes {
        rewrite_rollout_provider(change)?;
    }
    Ok(updated_rows)
}

fn move_threads_to_trash(
    home: &SessionHomeEntry,
    trash_root: &Path,
    snapshots: &[ThreadSnapshot],
) -> Result<(), String> {
    trash_snapshots_for_home(home, trash_root, snapshots)?;
    remove_rollout_files(snapshots)?;
    remove_threads_from_db(&home.data_dir, snapshots)?;
    rewrite_session_index_without_ids(&home.data_dir, snapshots)?;
    Ok(())
}

fn create_trash_root_dir(app_root: &Path) -> Result<PathBuf, String> {
    let root = app_root.join(SESSION_TRASH_ROOT_DIR);
    fs::create_dir_all(&root)
        .map_err(|error| format!("创建会话废纸篓目录失败 ({}): {}", root.display(), error))?;
    Ok(root)
}

fn trash_snapshots_for_home(
    home: &SessionHomeEntry,
    trash_root: &Path,
    snapshots: &[ThreadSnapshot],
) -> Result<(), String> {
    let deleted_at = now_rfc3339();
    let batch_dir = trash_root.join(format!("batch-{}", now_epoch_secs()));
    fs::create_dir_all(&batch_dir)
        .map_err(|error| format!("创建废纸篓批次目录失败 ({}): {}", batch_dir.display(), error))?;

    for snapshot in snapshots {
        let entry_dir = batch_dir.join(format!(
            "{}-{}",
            sanitize_for_file_name(&snapshot.id),
            sanitize_for_file_name(&home.id)
        ));
        let files_dir = entry_dir.join("files");
        let relative_rollout_path = snapshot
            .rollout_path
            .strip_prefix(&home.data_dir)
            .map_err(|_| format!("无法计算 rollout 相对路径: {}", snapshot.rollout_path.display()))?;
        let target_rollout_path = files_dir.join(relative_rollout_path);
        if let Some(parent) = target_rollout_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("创建废纸篓文件目录失败 ({}): {}", parent.display(), error)
            })?;
        }
        fs::copy(&snapshot.rollout_path, &target_rollout_path).map_err(|error| {
            format!(
                "复制会话文件到废纸篓失败 ({} -> {}): {}",
                snapshot.rollout_path.display(),
                target_rollout_path.display(),
                error
            )
        })?;

        let manifest = json!({
            "sessionId": snapshot.id,
            "title": snapshot.title,
            "cwd": snapshot.cwd,
            "homeId": home.id,
            "homeLabel": home.label,
            "homeRoot": home.data_dir,
            "originalRolloutPath": snapshot.rollout_path,
            "relativeRolloutPath": relative_rollout_path.to_string_lossy(),
            "sessionIndexEntry": snapshot.session_index_entry,
            "threadRow": serialize_row_data(&snapshot.row_data),
            "deletedAt": deleted_at,
        });
        fs::write(
            entry_dir.join("manifest.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&manifest)
                    .map_err(|error| format!("序列化会话废纸篓清单失败: {}", error))?
            ),
        )
        .map_err(|error| {
            format!(
                "写入会话废纸篓清单失败 ({}): {}",
                entry_dir.display(),
                error
            )
        })?;
    }

    Ok(())
}

fn remove_threads_from_db(root_dir: &Path, snapshots: &[ThreadSnapshot]) -> Result<(), String> {
    let db_path = root_dir.join(STATE_DB_FILE);
    let mut connection = Connection::open(&db_path)
        .map_err(|error| format!("打开实例数据库失败 ({}): {}", db_path.display(), error))?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("开启删除事务失败 ({}): {}", db_path.display(), error))?;

    for snapshot in snapshots {
        transaction
            .execute("DELETE FROM threads WHERE id = ?1", [&snapshot.id])
            .map_err(|error| format!("删除会话记录失败 ({}): {}", snapshot.id, error))?;
    }

    transaction
        .commit()
        .map_err(|error| format!("提交删除事务失败 ({}): {}", db_path.display(), error))?;
    Ok(())
}

fn remove_rollout_files(snapshots: &[ThreadSnapshot]) -> Result<(), String> {
    for snapshot in snapshots {
        if snapshot.rollout_path.exists() {
            fs::remove_file(&snapshot.rollout_path).map_err(|error| {
                format!(
                    "删除原始 rollout 文件失败 ({}): {}",
                    snapshot.rollout_path.display(),
                    error
                )
            })?;
            cleanup_empty_session_dirs(snapshot.rollout_path.parent());
        }
    }
    Ok(())
}

fn cleanup_empty_session_dirs(mut current: Option<&Path>) {
    while let Some(dir) = current {
        let file_name = dir.file_name().and_then(|item| item.to_str()).unwrap_or_default();
        if SESSION_DIRS.contains(&file_name) {
            break;
        }
        let is_empty = fs::read_dir(dir)
            .ok()
            .and_then(|mut entries| entries.next().transpose().ok())
            .flatten()
            .is_none();
        if !is_empty {
            break;
        }
        if fs::remove_dir(dir).is_err() {
            break;
        }
        current = dir.parent();
    }
}

fn rewrite_session_index_without_ids(root_dir: &Path, snapshots: &[ThreadSnapshot]) -> Result<(), String> {
    let path = root_dir.join(SESSION_INDEX_FILE);
    if !path.exists() {
        return Ok(());
    }
    let removed_ids = snapshots
        .iter()
        .map(|snapshot| snapshot.id.as_str())
        .collect::<HashSet<_>>();
    let content = fs::read_to_string(&path).map_err(|error| {
        format!("读取 session_index.jsonl 失败 ({}): {}", path.display(), error)
    })?;
    let retained = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return false;
            }
            match serde_json::from_str::<JsonValue>(trimmed) {
                Ok(value) => value
                    .get("id")
                    .and_then(JsonValue::as_str)
                    .map(|id| !removed_ids.contains(id))
                    .unwrap_or(true),
                Err(_) => true,
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let final_content = if retained.is_empty() {
        String::new()
    } else {
        format!("{}\n", retained)
    };
    fs::write(&path, final_content).map_err(|error| {
        format!("重写 session_index.jsonl 失败 ({}): {}", path.display(), error)
    })?;
    Ok(())
}

fn load_trash_entries(app_root: &Path) -> Result<Vec<TrashedSessionEntry>, String> {
    let root = create_trash_root_dir(app_root)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    let batch_dirs = fs::read_dir(&root)
        .map_err(|error| format!("读取会话废纸篓目录失败 ({}): {}", root.display(), error))?;
    for batch_dir in batch_dirs {
        let batch_dir = batch_dir.map_err(|error| {
            format!("读取会话废纸篓目录项失败 ({}): {}", root.display(), error)
        })?;
        let batch_path = batch_dir.path();
        if !batch_path.is_dir() {
            continue;
        }
        let entry_dirs = fs::read_dir(&batch_path).map_err(|error| {
            format!("读取会话废纸篓批次目录失败 ({}): {}", batch_path.display(), error)
        })?;
        for entry in entry_dirs {
            let entry = entry.map_err(|error| {
                format!("读取会话废纸篓条目失败 ({}): {}", batch_path.display(), error)
            })?;
            let entry_path = entry.path();
            if !entry_path.is_dir() {
                continue;
            }
            let manifest_path = entry_path.join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }
            let manifest_content = fs::read_to_string(&manifest_path).map_err(|error| {
                format!(
                    "读取会话废纸篓清单失败 ({}): {}",
                    manifest_path.display(),
                    error
                )
            })?;
            let manifest = serde_json::from_str::<TrashedSessionManifest>(&manifest_content)
                .map_err(|error| {
                    format!(
                        "解析会话废纸篓清单失败 ({}): {}",
                        manifest_path.display(),
                        error
                    )
                })?;
            let trashed_rollout_path = entry_path
                .join("files")
                .join(PathBuf::from(&manifest.relative_rollout_path));
            entries.push(TrashedSessionEntry {
                entry_dir: entry_path,
                manifest,
                trashed_rollout_path,
            });
        }
    }

    entries.sort_by(|left, right| {
        parse_deleted_at(right.manifest.deleted_at.as_deref())
            .unwrap_or_default()
            .cmp(&parse_deleted_at(left.manifest.deleted_at.as_deref()).unwrap_or_default())
            .then_with(|| left.manifest.session_id.cmp(&right.manifest.session_id))
    });
    Ok(entries)
}

fn restore_trashed_session_entry(entry: &TrashedSessionEntry) -> Result<(), String> {
    if !entry.trashed_rollout_path.exists() {
        return Err(format!(
            "废纸篓中的会话文件不存在，无法恢复 ({})",
            entry.manifest.session_id
        ));
    }

    let row_data = deserialize_row_data(&entry.manifest.thread_row)?;
    let session_id = row_data
        .get_text("id")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| entry.manifest.session_id.clone());
    let target_rollout_path = entry.manifest.original_rollout_path.clone();
    if target_rollout_path.exists() {
        return Err(format!("目标目录中已存在该会话，无法恢复 ({})", session_id));
    }

    let original_session_index_content = read_session_index_content(&entry.manifest.home_root)?;
    if session_index_contains_id(&original_session_index_content, &session_id)? {
        return Err(format!(
            "目标目录的 session_index.jsonl 中已存在该会话，无法恢复 ({})",
            session_id
        ));
    }

    if let Some(parent) = target_rollout_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建会话恢复目录失败 ({}): {}", parent.display(), error))?;
    }
    fs::copy(&entry.trashed_rollout_path, &target_rollout_path).map_err(|error| {
        format!(
            "恢复会话文件失败 ({} -> {}): {}",
            entry.trashed_rollout_path.display(),
            target_rollout_path.display(),
            error
        )
    })?;

    let restore_result = (|| {
        write_session_index_with_entry(
            &entry.manifest.home_root,
            &original_session_index_content,
            &session_id,
            &entry.manifest.session_index_entry,
        )?;
        insert_thread_row_from_json(&entry.manifest.home_root, &row_data)?;
        Ok::<(), String>(())
    })();

    if let Err(error) = restore_result {
        let _ = fs::remove_file(&target_rollout_path);
        let _ = restore_session_index_content(
            &entry.manifest.home_root,
            original_session_index_content.as_deref(),
        );
        return Err(error);
    }

    if let Err(error) = fs::remove_dir_all(&entry.entry_dir) {
        eprintln!(
            "warning: restored session but failed to remove trash entry {}: {}",
            entry.entry_dir.display(),
            error
        );
    }
    Ok(())
}

fn read_target_provider(data_dir: &Path) -> Result<String, String> {
    let config_path = data_dir.join("config.toml");
    if !config_path.exists() {
        return Ok("openai".to_string());
    }
    let content = fs::read_to_string(&config_path).map_err(|error| {
        format!("读取 config.toml 失败 ({}): {}", config_path.display(), error)
    })?;
    if content.trim().is_empty() {
        return Ok("openai".to_string());
    }
    let parsed = content.parse::<toml::Value>().map_err(|error| {
        format!("解析 config.toml 失败 ({}): {}", config_path.display(), error)
    })?;
    let provider = parsed
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("openai");
    Ok(provider.to_string())
}

#[derive(Debug, Clone)]
struct RolloutProviderChange {
    relative_path: PathBuf,
    absolute_path: PathBuf,
    updated_first_line: String,
}

fn collect_rollout_provider_changes(data_dir: &Path, target_provider: &str) -> Result<Vec<RolloutProviderChange>, String> {
    let mut changes = Vec::new();
    for dir_name in SESSION_DIRS {
        let root_dir = data_dir.join(dir_name);
        if !root_dir.exists() {
            continue;
        }
        let rollout_paths = list_rollout_files(&root_dir)?;
        for rollout_path in rollout_paths {
            let Some((first_line, separator)) = read_first_line(&rollout_path)? else {
                continue;
            };
            let Some(mut parsed) = parse_session_meta_record(&first_line) else {
                continue;
            };
            let current_provider = parsed["payload"]
                .get("model_provider")
                .and_then(JsonValue::as_str)
                .unwrap_or("");
            if current_provider == target_provider {
                continue;
            }
            if let Some(payload) = parsed.get_mut("payload").and_then(JsonValue::as_object_mut) {
                payload.insert(
                    "model_provider".to_string(),
                    JsonValue::String(target_provider.to_string()),
                );
            }
            let relative_path = rollout_path
                .strip_prefix(data_dir)
                .map_err(|_| format!("无法计算 rollout 相对路径: {}", rollout_path.display()))?;
            let updated_first_line = serde_json::to_string(&parsed)
                .map_err(|error| format!("序列化 session_meta 失败: {}", error))?;
            let full_line = format!("{updated_first_line}{separator}");
            changes.push(RolloutProviderChange {
                relative_path: relative_path.to_path_buf(),
                absolute_path: rollout_path,
                updated_first_line: full_line,
            });
        }
    }
    changes.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(changes)
}

fn count_sqlite_rows_to_update(data_dir: &Path, target_provider: &str) -> Result<usize, String> {
    let db_path = data_dir.join(STATE_DB_FILE);
    if !db_path.exists() {
        return Ok(0);
    }
    let connection = Connection::open(&db_path)
        .map_err(|error| format!("打开实例数据库失败 ({}): {}", db_path.display(), error))?;
    let count = connection
        .query_row(
            "SELECT COUNT(*) FROM threads WHERE COALESCE(model_provider, '') <> ?1",
            [target_provider],
            |row| row.get::<usize, i64>(0),
        )
        .map_err(|error| {
            format!(
                "统计 SQLite provider 差异失败 ({}): {}",
                db_path.display(),
                error
            )
        })?;
    Ok(count.max(0) as usize)
}

fn update_sqlite_provider(data_dir: &Path, target_provider: &str) -> Result<usize, String> {
    let db_path = data_dir.join(STATE_DB_FILE);
    if !db_path.exists() {
        return Ok(0);
    }
    let mut connection = Connection::open(&db_path)
        .map_err(|error| format!("打开实例数据库失败 ({}): {}", db_path.display(), error))?;
    connection
        .busy_timeout(Duration::from_secs(3))
        .map_err(|error| format!("设置 SQLite busy_timeout 失败 ({}): {}", db_path.display(), error))?;
    let transaction = connection
        .transaction()
        .map_err(|error| format_sqlite_write_error(&db_path, &error))?;
    let updated_rows = transaction
        .execute(
            "UPDATE threads SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1",
            [target_provider],
        )
        .map_err(|error| format_sqlite_write_error(&db_path, &error))?;
    transaction
        .commit()
        .map_err(|error| format_sqlite_write_error(&db_path, &error))?;
    Ok(updated_rows)
}

fn rewrite_rollout_provider(change: &RolloutProviderChange) -> Result<(), String> {
    let bytes = fs::read(&change.absolute_path).map_err(|error| {
        format!("读取 rollout 文件失败 ({}): {}", change.absolute_path.display(), error)
    })?;
    let (_, first_line_len) = detect_first_line_end(&bytes);
    let mut next_bytes = Vec::with_capacity(change.updated_first_line.len() + bytes.len());
    next_bytes.extend_from_slice(change.updated_first_line.as_bytes());
    next_bytes.extend_from_slice(&bytes[first_line_len..]);
    write_bytes_atomic(&change.absolute_path, &next_bytes)
}

fn backup_sync_files(data_dir: &Path) -> Result<PathBuf, String> {
    let backup_dir = data_dir.join(format!("backup-{}-session-sync", now_epoch_secs()));
    fs::create_dir_all(&backup_dir)
        .map_err(|error| format!("创建备份目录失败 ({}): {}", backup_dir.display(), error))?;
    for file_name in BACKUP_FILE_NAMES {
        let source = data_dir.join(file_name);
        if !source.exists() {
            continue;
        }
        let target = backup_dir.join(format!("{file_name}.bak"));
        fs::copy(&source, &target).map_err(|error| {
            format!("备份文件失败 ({} -> {}): {}", source.display(), target.display(), error)
        })?;
    }
    Ok(backup_dir)
}

fn backup_visibility_files(
    data_dir: &Path,
    rollout_changes: &[RolloutProviderChange],
    include_sqlite: bool,
) -> Result<PathBuf, String> {
    let backup_dir = data_dir.join(format!("backup-{}-session-visibility", now_epoch_secs()));
    fs::create_dir_all(&backup_dir)
        .map_err(|error| format!("创建备份目录失败 ({}): {}", backup_dir.display(), error))?;

    for change in rollout_changes {
        let target = backup_dir.join("files").join(&change.relative_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("创建 rollout 备份目录失败 ({}): {}", parent.display(), error)
            })?;
        }
        fs::copy(&change.absolute_path, &target).map_err(|error| {
            format!(
                "备份 rollout 文件失败 ({} -> {}): {}",
                change.absolute_path.display(),
                target.display(),
                error
            )
        })?;
    }

    if include_sqlite {
        let db_path = data_dir.join(STATE_DB_FILE);
        if db_path.exists() {
            let target = backup_dir.join(STATE_DB_FILE);
            fs::copy(&db_path, &target).map_err(|error| {
                format!(
                    "备份 state_5.sqlite 失败 ({} -> {}): {}",
                    db_path.display(),
                    target.display(),
                    error
                )
            })?;
        }
    }

    Ok(backup_dir)
}

fn restore_visibility_files_from_backup(data_dir: &Path, backup_dir: &Path, include_sqlite: bool) -> Result<(), String> {
    let files_root = backup_dir.join("files");
    if files_root.exists() {
        restore_directory_contents(&files_root, data_dir)?;
    }

    if include_sqlite {
        let backup_db_path = backup_dir.join(STATE_DB_FILE);
        if backup_db_path.exists() {
            let target_db_path = data_dir.join(STATE_DB_FILE);
            fs::copy(&backup_db_path, &target_db_path).map_err(|error| {
                format!(
                    "恢复 state_5.sqlite 失败 ({} -> {}): {}",
                    backup_db_path.display(),
                    target_db_path.display(),
                    error
                )
            })?;
        }
    }
    Ok(())
}

fn restore_directory_contents(source_root: &Path, target_root: &Path) -> Result<(), String> {
    let entries = fs::read_dir(source_root)
        .map_err(|error| format!("读取备份目录失败 ({}): {}", source_root.display(), error))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("读取备份目录项失败 ({}): {}", source_root.display(), error)
        })?;
        let source_path = entry.path();
        let target_path = target_root.join(
            source_path
                .strip_prefix(source_root)
                .map_err(|_| format!("无法计算备份相对路径: {}", source_path.display()))?,
        );
        if entry
            .file_type()
            .map_err(|error| format!("读取文件类型失败 ({}): {}", source_path.display(), error))?
            .is_dir()
        {
            fs::create_dir_all(&target_path)
                .map_err(|error| format!("创建恢复目录失败 ({}): {}", target_path.display(), error))?;
            restore_directory_contents(&source_path, &target_path)?;
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("创建恢复父目录失败 ({}): {}", parent.display(), error)
            })?;
        }
        fs::copy(&source_path, &target_path).map_err(|error| {
            format!(
                "恢复备份文件失败 ({} -> {}): {}",
                source_path.display(),
                target_path.display(),
                error
            )
        })?;
    }
    Ok(())
}

fn read_session_index_map(root_dir: &Path) -> Result<HashMap<String, JsonValue>, String> {
    let path = root_dir.join(SESSION_INDEX_FILE);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(&path).map_err(|error| {
        format!("读取 session_index.jsonl 失败 ({}): {}", path.display(), error)
    })?;
    let mut entries = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<JsonValue>(trimmed) else {
            continue;
        };
        let Some(id) = parsed.get("id").and_then(JsonValue::as_str) else {
            continue;
        };
        entries.insert(id.to_string(), parsed);
    }
    Ok(entries)
}

fn build_fallback_session_index_entry(id: &str, title: &str, updated_at: Option<i64>) -> JsonValue {
    let mut value = json!({
        "id": id,
        "thread_name": title,
    });
    if let Some(updated_at) = updated_at {
        value["updated_at"] = JsonValue::String(updated_at.to_string());
    }
    value
}

fn append_session_index_entries(
    root_dir: &Path,
    existing_ids: &HashSet<String>,
    snapshots: &[ThreadSnapshot],
) -> Result<(), String> {
    let path = root_dir.join(SESSION_INDEX_FILE);
    let mut lines = Vec::new();
    for snapshot in snapshots {
        if existing_ids.contains(&snapshot.id) {
            continue;
        }
        lines.push(
            serde_json::to_string(&snapshot.session_index_entry)
                .map_err(|error| format!("序列化 session_index 条目失败: {}", error))?,
        );
    }
    if lines.is_empty() {
        return Ok(());
    }

    let needs_prefix = path.exists() && !file_ends_with_newline(&path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("打开 session_index.jsonl 失败 ({}): {}", path.display(), error))?;

    if needs_prefix {
        file.write_all(b"\n").map_err(|error| {
            format!("写入 session_index 换行失败 ({}): {}", path.display(), error)
        })?;
    }

    for line in lines {
        file.write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|error| {
                format!("追加 session_index 条目失败 ({}): {}", path.display(), error)
            })?;
    }
    Ok(())
}

fn update_global_state<'a>(root_dir: &Path, workspaces: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let path = root_dir.join(GLOBAL_STATE_FILE);
    let mut value = if path.exists() {
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("读取全局状态失败 ({}): {}", path.display(), error))?;
        serde_json::from_str::<JsonValue>(&raw).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    if !value.is_object() {
        value = json!({});
    }

    let Some(object) = value.as_object_mut() else {
        return Err("全局状态文件格式无效".to_string());
    };

    let unique_workspaces = workspaces
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.to_string())
        .collect::<HashSet<_>>();
    merge_string_array(object, "project-order", &unique_workspaces);
    merge_string_array(object, "electron-saved-workspace-roots", &unique_workspaces);

    let serialized = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("序列化全局状态失败: {}", error))?;
    fs::write(&path, format!("{}\n", serialized))
        .map_err(|error| format!("写入全局状态失败 ({}): {}", path.display(), error))?;
    Ok(())
}

fn merge_string_array(
    object: &mut serde_json::Map<String, JsonValue>,
    key: &str,
    additions: &HashSet<String>,
) {
    let mut values = object
        .get(key)
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.as_str().map(|value| value.to_string()))
        .collect::<Vec<_>>();
    for addition in additions {
        if !values.contains(addition) {
            values.push(addition.clone());
        }
    }
    object.insert(
        key.to_string(),
        JsonValue::Array(values.into_iter().map(JsonValue::String).collect()),
    );
}

fn copy_rollout_file(snapshot: &ThreadSnapshot, target_root: &Path) -> Result<PathBuf, String> {
    let relative_path = snapshot
        .rollout_path
        .strip_prefix(&snapshot.source_root)
        .map_err(|_| {
            format!(
                "会话 {} 的 rollout 路径不在目录下: {}",
                snapshot.id,
                snapshot.rollout_path.display()
            )
        })?;
    let target_path = target_root.join(relative_path);
    let parent = target_path
        .parent()
        .ok_or_else(|| format!("无法解析目标 rollout 父目录: {}", target_path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("创建 rollout 目录失败 ({}): {}", parent.display(), error))?;
    fs::copy(&snapshot.rollout_path, &target_path).map_err(|error| {
        format!(
            "复制 rollout 文件失败 ({} -> {}): {}",
            snapshot.rollout_path.display(),
            target_path.display(),
            error
        )
    })?;
    Ok(target_path)
}

fn insert_thread_row(transaction: &Transaction<'_>, target_columns: &[String], row_data: &ThreadRowData) -> Result<(), String> {
    let mut columns = Vec::new();
    let mut values = Vec::new();
    for column in target_columns {
        if let Some(value) = row_data.get_value(column) {
            columns.push(quote_identifier(column));
            values.push(to_sql_literal(value));
        }
    }
    if columns.is_empty() {
        return Err("没有可写入的 threads 列".to_string());
    }
    let sql = format!(
        "INSERT OR REPLACE INTO threads ({}) VALUES ({})",
        columns.join(", "),
        values.join(", ")
    );
    transaction
        .execute(&sql, [])
        .map_err(|error| format!("写入 threads 表失败: {}", error))?;
    Ok(())
}

fn insert_thread_row_from_json(root_dir: &Path, row_data: &ThreadRowData) -> Result<(), String> {
    let db_path = root_dir.join(STATE_DB_FILE);
    if !db_path.exists() {
        return Err(format!("目标目录缺少 state_5.sqlite，无法恢复会话 ({})", db_path.display()));
    }
    let mut connection = Connection::open(&db_path)
        .map_err(|error| format!("打开实例数据库失败 ({}): {}", db_path.display(), error))?;
    connection
        .busy_timeout(Duration::from_secs(3))
        .map_err(|error| format!("设置数据库 busy_timeout 失败 ({}): {}", db_path.display(), error))?;
    let target_columns = read_thread_columns(&connection)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("开启会话恢复事务失败 ({}): {}", db_path.display(), error))?;

    let session_id = row_data
        .get_text("id")
        .filter(|value| !value.trim().is_empty())
        .ok_or("废纸篓中的线程数据缺少 id 字段".to_string())?;
    let exists = transaction
        .query_row(
            "SELECT COUNT(*) FROM threads WHERE id = ?1",
            [&session_id],
            |row| row.get::<usize, i64>(0),
        )
        .map_err(|error| format!("检查会话是否已存在失败 ({}): {}", session_id, error))?;
    if exists > 0 {
        return Err(format!("目标目录中已存在该会话，无法恢复 ({})", session_id));
    }

    insert_thread_row(&transaction, &target_columns, row_data)?;
    transaction
        .commit()
        .map_err(|error| format!("提交会话恢复事务失败 ({}): {}", db_path.display(), error))?;
    Ok(())
}

fn read_session_index_content(root_dir: &Path) -> Result<Option<String>, String> {
    let path = root_dir.join(SESSION_INDEX_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|error| {
        format!("读取 session_index.jsonl 失败 ({}): {}", path.display(), error)
    })?;
    Ok(Some(content))
}

fn session_index_contains_id(content: &Option<String>, session_id: &str) -> Result<bool, String> {
    let Some(content) = content.as_deref() else {
        return Ok(false);
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = serde_json::from_str::<JsonValue>(trimmed)
            .map_err(|error| format!("解析 session_index.jsonl 条目失败: {}", error))?;
        if parsed.get("id").and_then(JsonValue::as_str) == Some(session_id) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn write_session_index_with_entry(
    root_dir: &Path,
    original_content: &Option<String>,
    session_id: &str,
    entry: &JsonValue,
) -> Result<(), String> {
    let path = root_dir.join(SESSION_INDEX_FILE);
    let serialized_entry = serde_json::to_string(entry)
        .map_err(|error| format!("序列化 session_index 条目失败 ({}): {}", session_id, error))?;
    let mut lines = original_content
        .as_deref()
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    lines.push(serialized_entry);
    let next_content = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };
    fs::write(&path, next_content).map_err(|error| {
        format!("写入 session_index.jsonl 失败 ({}): {}", path.display(), error)
    })?;
    Ok(())
}

fn restore_session_index_content(root_dir: &Path, content: Option<&str>) -> Result<(), String> {
    let path = root_dir.join(SESSION_INDEX_FILE);
    match content {
        Some(value) => fs::write(&path, value).map_err(|error| {
            format!("恢复 session_index.jsonl 失败 ({}): {}", path.display(), error)
        })?,
        None => {
            if path.exists() {
                fs::remove_file(&path).map_err(|error| {
                    format!(
                        "删除恢复失败的 session_index.jsonl 失败 ({}): {}",
                        path.display(),
                        error
                    )
                })?;
            }
        }
    }
    Ok(())
}

fn deserialize_row_data(value: &JsonValue) -> Result<ThreadRowData, String> {
    let object = value
        .as_object()
        .ok_or("废纸篓中的线程数据格式无效，缺少对象结构".to_string())?;
    let mut columns = object.keys().cloned().collect::<Vec<_>>();
    columns.sort();
    let values = columns
        .iter()
        .map(|column| json_to_sqlite_value(object.get(column).unwrap_or(&JsonValue::Null)))
        .collect::<Vec<_>>();
    Ok(ThreadRowData { columns, values })
}

fn serialize_row_data(row_data: &ThreadRowData) -> JsonValue {
    let mut object = serde_json::Map::new();
    for (column, value) in row_data.columns.iter().zip(row_data.values.iter()) {
        object.insert(column.clone(), sqlite_value_to_json(value));
    }
    JsonValue::Object(object)
}

fn json_to_sqlite_value(value: &JsonValue) -> Value {
    match value {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(flag) => Value::Integer(i64::from(*flag)),
        JsonValue::Number(number) => number
            .as_i64()
            .map(Value::Integer)
            .or_else(|| number.as_f64().map(Value::Real))
            .unwrap_or_else(|| Value::Text(number.to_string())),
        JsonValue::String(text) => Value::Text(text.clone()),
        JsonValue::Array(_) | JsonValue::Object(_) => Value::Text(value.to_string()),
    }
}

fn sqlite_value_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Null => JsonValue::Null,
        Value::Integer(number) => json!(number),
        Value::Real(number) => json!(number),
        Value::Text(text) => json!(text),
        Value::Blob(bytes) => json!(bytes
            .iter()
            .map(|byte| format!("{:02X}", byte))
            .collect::<String>()),
    }
}

fn list_rollout_files(root_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut result = Vec::new();
    let entries = fs::read_dir(root_dir)
        .map_err(|error| format!("读取目录失败 ({}): {}", root_dir.display(), error))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("读取目录项失败 ({}): {}", root_dir.display(), error))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("读取文件类型失败 ({}): {}", path.display(), error))?;
        if file_type.is_dir() {
            result.extend(list_rollout_files(&path)?);
            continue;
        }
        if file_type.is_file() {
            let file_name = path.file_name().and_then(|item| item.to_str()).unwrap_or_default();
            if file_name.starts_with("rollout-") && file_name.ends_with(".jsonl") {
                result.push(path);
            }
        }
    }
    result.sort();
    Ok(result)
}

fn read_first_line(path: &Path) -> Result<Option<(String, String)>, String> {
    let file =
        File::open(path).map_err(|error| format!("打开 rollout 文件失败 ({}): {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    let bytes_read = reader
        .read_until(b'\n', &mut buffer)
        .map_err(|error| format!("读取 rollout 首行失败 ({}): {}", path.display(), error))?;
    if bytes_read == 0 {
        return Ok(None);
    }

    let (line_bytes, separator) = if buffer.ends_with(b"\r\n") {
        (&buffer[..buffer.len() - 2], "\r\n")
    } else if buffer.ends_with(b"\n") {
        (&buffer[..buffer.len() - 1], "\n")
    } else {
        (&buffer[..], "")
    };
    let line = String::from_utf8(line_bytes.to_vec()).map_err(|error| {
        format!("解析 rollout 首行 UTF-8 失败 ({}): {}", path.display(), error)
    })?;
    Ok(Some((line, separator.to_string())))
}

fn parse_session_meta_record(first_line: &str) -> Option<JsonValue> {
    if first_line.trim().is_empty() {
        return None;
    }
    let parsed = serde_json::from_str::<JsonValue>(first_line).ok()?;
    if parsed.get("type").and_then(JsonValue::as_str) != Some("session_meta") {
        return None;
    }
    if !parsed.get("payload").is_some_and(JsonValue::is_object) {
        return None;
    }
    Some(parsed)
}

fn detect_first_line_end(bytes: &[u8]) -> (&'static str, usize) {
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            if index > 0 && bytes[index - 1] == b'\r' {
                return ("\r\n", index + 1);
            }
            return ("\n", index + 1);
        }
    }
    ("", bytes.len())
}

fn write_bytes_atomic(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("无法定位目标目录: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("创建目录失败 ({}): {}", parent.display(), error))?;
    let temp_path = parent.join(format!(
        ".{}.session-manager.{}.tmp",
        path.file_name()
            .and_then(|item| item.to_str())
            .unwrap_or("file"),
        now_epoch_secs()
    ));
    fs::write(&temp_path, content)
        .map_err(|error| format!("写入临时文件失败 ({}): {}", temp_path.display(), error))?;
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("替换文件失败 ({}): {}", path.display(), error));
    }
    Ok(())
}

fn open_readonly_connection(db_path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("打开只读数据库失败 ({}): {}", db_path.display(), error))
}

fn read_thread_columns(connection: &Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(threads)")
        .map_err(|error| format!("读取 threads 表结构失败: {}", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| format!("查询 threads 表结构失败: {}", error))?;
    let mut columns = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("解析 threads 表结构失败: {}", error))?
    {
        columns.push(
            row.get::<usize, String>(1)
                .map_err(|error| format!("解析 threads 列失败: {}", error))?,
        );
    }
    if columns.is_empty() {
        return Err("threads 表不存在或没有列定义".to_string());
    }
    Ok(columns)
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn to_sql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Integer(number) => number.to_string(),
        Value::Real(number) => {
            if number.is_finite() {
                number.to_string()
            } else {
                "NULL".to_string()
            }
        }
        Value::Text(text) => format!("'{}'", text.replace('\'', "''")),
        Value::Blob(bytes) => format!(
            "X'{}'",
            bytes
                .iter()
                .map(|byte| format!("{:02X}", byte))
                .collect::<String>()
        ),
    }
}

fn file_ends_with_newline(path: &Path) -> Result<bool, String> {
    let bytes = fs::read(path).map_err(|error| format!("读取文件失败 ({}): {}", path.display(), error))?;
    Ok(bytes.is_empty() || bytes.last() == Some(&b'\n'))
}

fn format_sqlite_write_error(path: &Path, error: &rusqlite::Error) -> String {
    let message = error.to_string();
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("database is locked") || lowered.contains("database busy") {
        return format!(
            "state_5.sqlite 当前被占用，请关闭 Codex 后重试 ({}): {}",
            path.display(),
            message
        );
    }
    format!("更新 SQLite 失败 ({}): {}", path.display(), message)
}

fn sanitize_for_file_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn parse_deleted_at(value: Option<&str>) -> Option<i64> {
    let parsed = value.and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())?;
    Some(parsed.timestamp())
}

#[cfg(test)]
mod tests {
    use super::{
        get_session_token_stats, list_session_groups, list_trashed_sessions, move_sessions_to_trash,
        repair_session_visibility, restore_sessions_from_trash,
    };
    use rusqlite::Connection;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn session_groups_merge_locations_and_token_stats() {
        let temp = tempdir().unwrap();
        let app_root = temp.path();
        let official = app_root.join("official-home");
        write_test_home(&official, "session-1", "项目A", "D:/workspace/a", "openai").unwrap();
        std::env::set_var("CODEX_HOME", &official);

        let runtime = app_root.join("runtime").join("sess-2");
        fs::create_dir_all(&runtime).unwrap();
        fs::write(
            runtime.join("runtime-session.json"),
            r#"{"session_id":"sess-2","profile_key":"platform-desktop","target":"desktop","created_at_epoch_secs":1}"#,
        )
        .unwrap();
        write_test_home(
            &runtime.join("platform-desktop"),
            "session-1",
            "项目A",
            "D:/workspace/a",
            "openai",
        )
        .unwrap();

        let groups = list_session_groups(app_root).unwrap();
        let stats = get_session_token_stats(app_root, &[String::from("session-1")]).unwrap();
        std::env::remove_var("CODEX_HOME");

        assert!(groups
            .iter()
            .flat_map(|group| group.sessions.iter())
            .any(|session| session.session_id == "session-1"));
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].total_tokens, 12);
    }

    #[test]
    fn move_to_trash_and_restore_round_trip_session() {
        let temp = tempdir().unwrap();
        let app_root = temp.path();
        let official = app_root.join("official-home");
        write_test_home(&official, "session-2", "项目B", "D:/workspace/b", "openai").unwrap();
        std::env::set_var("CODEX_HOME", &official);

        let moved = move_sessions_to_trash(app_root, &[String::from("session-2")]).unwrap();
        assert_eq!(moved.trashed_session_count, 1);
        let trashed = list_trashed_sessions(app_root).unwrap();
        assert_eq!(trashed.len(), 1);

        let restored =
            restore_sessions_from_trash(app_root, &[String::from("session-2")]).unwrap();
        let trashed_after_restore = list_trashed_sessions(app_root).unwrap();
        std::env::remove_var("CODEX_HOME");

        assert_eq!(restored.restored_session_count, 1);
        assert!(trashed_after_restore.is_empty());
    }

    #[test]
    fn repair_visibility_updates_rollout_and_sqlite_provider() {
        let temp = tempdir().unwrap();
        let app_root = temp.path();
        let official = app_root.join("official-home");
        write_test_home(&official, "session-3", "项目C", "D:/workspace/c", "openai").unwrap();
        fs::write(
            official.join("config.toml"),
            "model_provider = \"CustomOpenAI\"\n",
        )
        .unwrap();
        std::env::set_var("CODEX_HOME", &official);

        let summary = repair_session_visibility(app_root).unwrap();
        let rollout = fs::read_to_string(
            official
                .join("sessions")
                .join("D_workspace_c")
                .join("rollout-session-3.jsonl"),
        )
        .unwrap();
        std::env::remove_var("CODEX_HOME");

        assert_eq!(summary.mutated_home_count, 1);
        assert!(rollout.contains("\"model_provider\":\"CustomOpenAI\""));
    }

    fn write_test_home(
        home: &std::path::Path,
        session_id: &str,
        title: &str,
        cwd: &str,
        provider: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(home)?;
        fs::write(home.join("config.toml"), format!("model_provider = \"{provider}\"\n"))?;

        let db_path = home.join("state_5.sqlite");
        let connection = Connection::open(&db_path)?;
        connection.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, cwd TEXT, rollout_path TEXT, updated_at INTEGER, model_provider TEXT)",
            [],
        )?;

        let rollout_dir = home.join("sessions").join(sanitize_path_for_test(cwd));
        fs::create_dir_all(&rollout_dir)?;
        let rollout_path = rollout_dir.join(format!("rollout-{session_id}.jsonl"));
        fs::write(
            &rollout_path,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"model_provider\":\"{provider}\"}}}}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":5,\"output_tokens\":7,\"total_tokens\":12}}}}}}}}\n"
            ),
        )?;

        connection.execute(
            "INSERT INTO threads (id, title, cwd, rollout_path, updated_at, model_provider) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                session_id,
                title,
                cwd,
                rollout_path.to_string_lossy().to_string(),
                1_725_000_000_i64,
                provider,
            ),
        )?;

        fs::write(
            home.join("session_index.jsonl"),
            format!(
                "{{\"id\":\"{session_id}\",\"thread_name\":\"{title}\",\"updated_at\":\"1725000000\"}}\n"
            ),
        )?;
        Ok(())
    }

    fn sanitize_path_for_test(value: &str) -> String {
        value.replace(':', "").replace('\\', "_").replace('/', "_")
    }
}
