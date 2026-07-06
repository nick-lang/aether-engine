//! Filesystem-backed canonical store.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use std::collections::{HashMap, HashSet};

use aether_core::{EngineError, EngineResult};
use aether_package::{
    apply_patch_to_package, load_package, patch_invalid_for_package, patch_is_noop_for_package,
    prune_orphan_tile_layers, validate_package, GamePackage,
};
use aether_patch::PatchDocument;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
pub struct PruneRequest {
    #[serde(default = "prune_default_true")]
    pub orphan_assets: bool,
    #[serde(default)]
    pub clear_archived_candidates: bool,
    #[serde(default)]
    pub clear_publish_history: bool,
    #[serde(default)]
    pub clear_pending_candidates: bool,
}

fn prune_default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct PruneReport {
    pub assets_removed: Vec<String>,
    pub archived_candidates_removed: u64,
    pub published_records_removed: u64,
    pub pending_candidates_removed: u64,
}

pub fn referenced_asset_ids(package: &GamePackage) -> HashSet<String> {
    let mut ids = HashSet::new();
    for entity in &package.entities {
        if let Some(sprite) = &entity.components.sprite {
            if !sprite.asset.trim().is_empty() {
                ids.insert(sprite.asset.clone());
            }
        }
    }
    ids
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub package_id: String,
    pub package_version: u64,
    /// Monotonic audit / SSE sequence (never decremented on rollback).
    pub published_sequence: u64,
    /// Patch ids still applied to canonical, in apply order.
    #[serde(default)]
    pub active_patches: Vec<String>,
    /// Display names for patches (audit `patch_id` stays stable for re-apply).
    #[serde(default)]
    pub patch_labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResult {
    pub removed_patch_id: String,
    /// Other active patches removed because they depended on the rolled-back change.
    #[serde(default)]
    pub cascaded_patch_ids: Vec<String>,
    pub active_patches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletePatchResult {
    pub patch_id: String,
    pub assets_removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactivateResult {
    pub patch_id: String,
    pub active_patches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    Pending,
    Published,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateRecord {
    pub patch_id: String,
    pub status: CandidateStatus,
    pub patch: PatchDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedPatch {
    pub sequence: u64,
    pub patch: PatchDocument,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone)]
pub struct PlaneStore {
    root: PathBuf,
    inner: Arc<Mutex<()>>,
}

impl PlaneStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            inner: Arc::new(Mutex::new(())),
        }
    }

    fn game_dir(&self, game_id: &str) -> PathBuf {
        self.root.join(game_id)
    }

    fn with_lock<R>(&self, f: impl FnOnce() -> EngineResult<R>) -> EngineResult<R> {
        let _guard = self
            .inner
            .lock()
            .map_err(|_| EngineError::InvalidPackage("store lock poisoned".into()))?;
        f()
    }

    pub fn seed_package(&self, game_id: &str, package_path: &Path) -> EngineResult<Manifest> {
        self.with_lock(|| {
            let package = load_package(package_path)?;
            if package.meta.id != game_id {
                return Err(EngineError::InvalidPackage(format!(
                    "package meta.id '{}' does not match game id '{}'",
                    package.meta.id, game_id
                )));
            }
            validate_package(&package)?;

            let dir = self.game_dir(game_id);
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(io_err)?;
            }
            fs::create_dir_all(&dir).map_err(io_err)?;
            fs::create_dir_all(dir.join("candidates")).map_err(io_err)?;
            fs::create_dir_all(dir.join("published")).map_err(io_err)?;

            let canonical = dir.join("canonical/package.json");
            if let Some(parent) = canonical.parent() {
                fs::create_dir_all(parent).map_err(io_err)?;
            }
            let json = serde_json::to_string_pretty(&package)
                .map_err(|e| EngineError::InvalidPackage(e.to_string()))?;
            fs::write(&canonical, json).map_err(io_err)?;
            write_base_package(&dir, &package)?;

            let manifest = Manifest {
                package_id: game_id.to_string(),
                package_version: package.meta.version,
                published_sequence: 0,
                active_patches: Vec::new(),
                patch_labels: HashMap::new(),
            };
            write_manifest(&dir, &manifest)?;
            Ok(manifest)
        })
    }

    pub fn get_manifest(&self, game_id: &str) -> EngineResult<Manifest> {
        self.with_lock(|| {
            let game_dir = self.game_dir(game_id);
            reconcile_manifest(&game_dir)?;
            read_manifest(&game_dir)
        })
    }

    /// Remove all plane data for a game (published history, candidates, canonical).
    pub fn reset_game(&self, game_id: &str) -> EngineResult<()> {
        self.with_lock(|| {
            let dir = self.game_dir(game_id);
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(io_err)?;
            }
            Ok(())
        })
    }

    pub fn asset_path(&self, game_id: &str, asset_id: &str) -> PathBuf {
        self.game_dir(game_id).join("assets").join(format!("{asset_id}.png"))
    }

    pub fn assets_dir(&self, game_id: &str) -> PathBuf {
        self.game_dir(game_id).join("assets")
    }

    fn traces_dir(&self, game_id: &str) -> PathBuf {
        self.game_dir(game_id).join("traces")
    }

    /// Persist an LLM generation trace for offline debugging (Ollama/Dream/OpenAI).
    pub fn save_generation_trace(
        &self,
        game_id: &str,
        prompt: &str,
        provider: &str,
        patch_id: &str,
        trace: &crate::generation::GenerationTrace,
    ) -> EngineResult<PathBuf> {
        use crate::generation::{trace_file_name, unix_ms_now, PersistedGenerationTrace};

        self.with_lock(|| {
            let dir = self.traces_dir(game_id);
            fs::create_dir_all(&dir).map_err(io_err)?;
            let saved_at_ms = unix_ms_now();
            let record = PersistedGenerationTrace {
                saved_at_ms,
                game_id: game_id.to_string(),
                patch_id: patch_id.to_string(),
                prompt: prompt.to_string(),
                provider: provider.to_string(),
                trace: trace.clone(),
            };
            let path = dir.join(trace_file_name(saved_at_ms, patch_id));
            write_json(&path, &record)?;
            Ok(path)
        })
    }

    /// List persisted traces for a game, newest first.
    pub fn list_generation_traces(
        &self,
        game_id: &str,
    ) -> EngineResult<Vec<crate::generation::PersistedGenerationTrace>> {
        use crate::generation::PersistedGenerationTrace;

        self.with_lock(|| {
            let dir = self.traces_dir(game_id);
            if !dir.is_dir() {
                return Ok(Vec::new());
            }
            let mut traces = Vec::new();
            for entry in fs::read_dir(&dir).map_err(io_err)? {
                let entry = entry.map_err(io_err)?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(record) = read_json::<PersistedGenerationTrace>(&path) {
                    traces.push(record);
                }
            }
            traces.sort_by(|a, b| b.saved_at_ms.cmp(&a.saved_at_ms));
            Ok(traces)
        })
    }

    pub fn get_canonical_package(&self, game_id: &str) -> EngineResult<GamePackage> {
        self.with_lock(|| {
            let path = self.game_dir(game_id).join("canonical/package.json");
            load_package(path)
        })
    }

    /// Replace canonical with a full generated game. Clears pending candidates and records audit history.
    pub fn replace_canonical_package(
        &self,
        game_id: &str,
        package: GamePackage,
        audit_patch: PatchDocument,
    ) -> EngineResult<PublishedPatch> {
        self.with_lock(|| {
            if package.meta.id != game_id {
                return Err(EngineError::InvalidPackage(format!(
                    "package meta.id '{}' does not match game id '{}'",
                    package.meta.id, game_id
                )));
            }
            validate_package(&package)?;

            let game_dir = self.game_dir(game_id);
            let mut manifest = read_manifest(&game_dir)?;
            write_canonical(&game_dir, &package)?;
            write_base_package(&game_dir, &package)?;
            manifest.active_patches.clear();

            let cand_dir = game_dir.join("candidates");
            if cand_dir.exists() {
                for entry in fs::read_dir(&cand_dir).map_err(io_err)? {
                    let path = entry.map_err(io_err)?.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("json") {
                        let _ = fs::remove_file(path);
                    }
                }
            }

            manifest.published_sequence += 1;
            let published = PublishedPatch {
                sequence: manifest.published_sequence,
                patch: audit_patch,
                label: None,
            };
            let pub_path = game_dir.join("published").join(format!(
                "{:06}_{}.json",
                published.sequence, published.patch.patch_id
            ));
            fs::create_dir_all(game_dir.join("published")).map_err(io_err)?;
            write_json(&pub_path, &published)?;
            write_manifest(&game_dir, &manifest)?;
            Ok(published)
        })
    }

    pub fn submit_candidate(&self, game_id: &str, patch: PatchDocument) -> EngineResult<()> {
        self.with_lock(|| {
            let game_dir = self.game_dir(game_id);
            let manifest = read_manifest(&game_dir)?;
            patch.validate_version(manifest.package_version)?;
            crate::generation::validate_patch_rules(&patch)?;

            let canonical_path = game_dir.join("canonical/package.json");
            let package = load_package(&canonical_path)?;
            if patch_is_noop_for_package(&package, &patch)? {
                return Err(EngineError::PatchRejected(
                    "patch is a no-op — canonical package unchanged after apply".into(),
                ));
            }

            let dir = game_dir.join("candidates");
            fs::create_dir_all(&dir).map_err(io_err)?;
            let record = CandidateRecord {
                patch_id: patch.patch_id.clone(),
                status: CandidateStatus::Pending,
                patch,
            };
            let path = dir.join(format!("{}.json", record.patch_id));
            write_json(&path, &record)?;
            Ok(())
        })
    }

    pub fn list_candidates(
        &self,
        game_id: &str,
        status: Option<CandidateStatus>,
    ) -> EngineResult<Vec<CandidateRecord>> {
        self.with_lock(|| {
            let dir = self.game_dir(game_id).join("candidates");
            if !dir.exists() {
                return Ok(Vec::new());
            }
            let mut out = Vec::new();
            for entry in fs::read_dir(&dir).map_err(io_err)? {
                let entry = entry.map_err(io_err)?;
                if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let record: CandidateRecord = read_json(entry.path())?;
                if let Some(ref want) = status {
                    if !status_matches(&record.status, want) {
                        continue;
                    }
                }
                out.push(record);
            }
            out.sort_by(|a, b| a.patch_id.cmp(&b.patch_id));
            Ok(out)
        })
    }

    pub fn publish_candidate(&self, game_id: &str, patch_id: &str) -> EngineResult<PublishedPatch> {
        self.with_lock(|| {
            let game_dir = self.game_dir(game_id);
            let candidate_path = game_dir.join("candidates").join(format!("{patch_id}.json"));
            let mut record: CandidateRecord = read_json(&candidate_path)?;
            if matches!(record.status, CandidateStatus::Published) {
                return Err(EngineError::PatchRejected(format!(
                    "patch '{patch_id}' already published"
                )));
            }

            let mut manifest = read_manifest(&game_dir)?;
            record.patch.validate_version(manifest.package_version)?;

            let canonical_path = game_dir.join("canonical/package.json");
            let mut package = load_package(&canonical_path)?;
            if patch_is_noop_for_package(&package, &record.patch)? {
                return Err(EngineError::PatchRejected(
                    "patch is a no-op — canonical package unchanged after apply".into(),
                ));
            }
            apply_patch_to_package(&mut package, &record.patch)?;
            write_canonical(&game_dir, &package)?;
            manifest.active_patches.push(patch_id.to_string());

            record.status = CandidateStatus::Published;
            let archive_dir = game_dir.join("candidates/archived");
            fs::create_dir_all(&archive_dir).map_err(io_err)?;
            write_json(
                archive_dir.join(format!("{patch_id}.json")),
                &record,
            )?;
            let _ = fs::remove_file(&candidate_path);

            manifest.published_sequence += 1;
            let published = PublishedPatch {
                sequence: manifest.published_sequence,
                patch: record.patch.clone(),
                label: manifest.patch_labels.get(patch_id).cloned(),
            };
            let pub_path = game_dir
                .join("published")
                .join(format!("{:06}_{}.json", published.sequence, patch_id));
            write_json(&pub_path, &published)?;
            write_manifest(&game_dir, &manifest)?;
            Ok(published)
        })
    }

    pub fn reject_candidate(&self, game_id: &str, patch_id: &str) -> EngineResult<()> {
        self.with_lock(|| {
            let candidate_path = self
                .game_dir(game_id)
                .join("candidates")
                .join(format!("{patch_id}.json"));
            let mut record: CandidateRecord = read_json(&candidate_path)?;
            record.status = CandidateStatus::Rejected;
            write_json(&candidate_path, &record)?;
            Ok(())
        })
    }

    /// Remove a patch from the active set and refold canonical from `base.json`.
    /// `patch_id: None` removes the most recently applied active patch.
    pub fn rollback_patch(
        &self,
        game_id: &str,
        patch_id: Option<&str>,
    ) -> EngineResult<RollbackResult> {
        self.with_lock(|| rollback_patch_inner(&self.game_dir(game_id), patch_id))
    }

    pub fn rollback_last(&self, game_id: &str) -> EngineResult<RollbackResult> {
        self.rollback_patch(game_id, None)
    }

    /// Friendly display name; `patch_id` in audit files is unchanged.
    pub fn set_patch_label(
        &self,
        game_id: &str,
        patch_id: &str,
        label: &str,
    ) -> EngineResult<()> {
        self.with_lock(|| {
            let game_dir = self.game_dir(game_id);
            find_published_patch(&game_dir, patch_id).map_err(|_| {
                EngineError::PatchRejected(format!("no published patch '{patch_id}'"))
            })?;
            let mut manifest = read_manifest(&game_dir)?;
            let trimmed = label.trim();
            if trimmed.is_empty() {
                manifest.patch_labels.remove(patch_id);
            } else {
                manifest.patch_labels
                    .insert(patch_id.to_string(), trimmed.to_string());
            }
            update_published_labels(&game_dir, patch_id, manifest.patch_labels.get(patch_id))?;
            write_manifest(&game_dir, &manifest)?;
            Ok(())
        })
    }

    /// Re-apply a rolled-back patch from the audit log (append to active set, refold).
    pub fn reactivate_patch(&self, game_id: &str, patch_id: &str) -> EngineResult<ReactivateResult> {
        self.with_lock(|| reactivate_patch_inner(&self.game_dir(game_id), patch_id))
    }

    /// Remove audit + candidate records for a patch that is not active; prune orphan assets.
    pub fn delete_inactive_patch(
        &self,
        game_id: &str,
        patch_id: &str,
    ) -> EngineResult<DeletePatchResult> {
        self.with_lock(|| delete_inactive_patch_inner(&self.game_dir(game_id), patch_id))
    }

    /// Remove PNG assets not referenced by current canonical sprites.
    pub fn prune_orphan_assets(&self, game_id: &str) -> EngineResult<Vec<String>> {
        self.with_lock(|| {
            let package = load_package(self.game_dir(game_id).join("canonical/package.json"))?;
            Ok(prune_orphan_assets_in_dir(&self.assets_dir(game_id), &package))
        })
    }

    pub fn prune_workspace(&self, game_id: &str, req: &PruneRequest) -> EngineResult<PruneReport> {
        self.with_lock(|| {
            let game_dir = self.game_dir(game_id);
            let package = load_package(game_dir.join("canonical/package.json"))?;
            let mut report = PruneReport {
                assets_removed: vec![],
                archived_candidates_removed: 0,
                published_records_removed: 0,
                pending_candidates_removed: 0,
            };

            if req.orphan_assets {
                report.assets_removed = prune_orphan_assets_in_dir(&game_dir.join("assets"), &package);
            }
            if req.clear_pending_candidates {
                report.pending_candidates_removed =
                    remove_json_files_in(&game_dir.join("candidates"), true)?;
            }
            if req.clear_archived_candidates {
                report.archived_candidates_removed =
                    remove_json_files_in(&game_dir.join("candidates/archived"), false)?;
            }
            if req.clear_publish_history {
                report.published_records_removed =
                    remove_json_files_in(&game_dir.join("published"), false)?;
                let mut manifest = read_manifest(&game_dir)?;
                manifest.published_sequence = 0;
                manifest.active_patches.clear();
                let package = load_package(game_dir.join("canonical/package.json"))?;
                write_base_package(&game_dir, &package)?;
                write_manifest(&game_dir, &manifest)?;
            }

            Ok(report)
        })
    }

    pub fn list_published_since(
        &self,
        game_id: &str,
        since: u64,
    ) -> EngineResult<Vec<PublishedPatch>> {
        self.with_lock(|| {
            let dir = self.game_dir(game_id).join("published");
            if !dir.exists() {
                return Ok(Vec::new());
            }
            let mut out = Vec::new();
            for entry in fs::read_dir(&dir).map_err(io_err)? {
                let entry = entry.map_err(io_err)?;
                let published: PublishedPatch = read_json(entry.path())?;
                if published.sequence > since {
                    out.push(published);
                }
            }
            out.sort_by_key(|p| p.sequence);
            Ok(out)
        })
    }
}

/// Sync manifest with on-disk audit log; migrate legacy workspaces to `active_patches`.
fn reconcile_manifest(game_dir: &Path) -> EngineResult<()> {
    let manifest_path = game_dir.join("manifest.json");
    if !manifest_path.is_file() {
        return Ok(());
    }
    let mut manifest: Manifest = read_json(&manifest_path)?;
    let max_on_disk = max_published_sequence(game_dir);
    if max_on_disk > manifest.published_sequence {
        manifest.published_sequence = max_on_disk;
    }
    let had_base = game_dir.join("canonical/base.json").is_file();
    ensure_base_package(game_dir)?;
    // One-time legacy migration: infer active set from audit log when upgrading old workspaces.
    if !had_base && manifest.active_patches.is_empty() && max_on_disk > 0 {
        manifest.active_patches = list_published_patch_ids_in_order(game_dir)?;
        refold_canonical(game_dir, &manifest)?;
    }
    write_manifest(game_dir, &manifest)?;
    Ok(())
}

fn rollback_patch_inner(game_dir: &Path, patch_id: Option<&str>) -> EngineResult<RollbackResult> {
    let mut manifest = read_manifest(game_dir)?;
    ensure_base_package(game_dir)?;

    let removed_patch_id = match patch_id {
        Some(id) => {
            let pos = manifest
                .active_patches
                .iter()
                .position(|p| p == id)
                .ok_or_else(|| {
                    EngineError::PatchRejected(format!(
                        "patch '{id}' is not in active_patches (nothing to roll back)"
                    ))
                })?;
            manifest.active_patches.remove(pos);
            id.to_string()
        }
        None => manifest.active_patches.pop().ok_or_else(|| {
            EngineError::PatchRejected("no active patches to roll back".into())
        })?,
    };

    let mut cascaded_patch_ids = Vec::new();
    cascade_inactive_dependents(game_dir, &mut manifest, &mut cascaded_patch_ids)?;

    let package = refold_canonical(game_dir, &manifest)?;
    let _ = prune_orphan_assets_in_dir(&game_dir.join("assets"), &package);
    write_manifest(game_dir, &manifest)?;

    Ok(RollbackResult {
        removed_patch_id,
        cascaded_patch_ids,
        active_patches: manifest.active_patches.clone(),
    })
}

/// Remove active patches that no longer match the refolded world (e.g. moss after room rollback).
fn cascade_inactive_dependents(
    game_dir: &Path,
    manifest: &mut Manifest,
    cascaded: &mut Vec<String>,
) -> EngineResult<()> {
    loop {
        let package = refold_canonical(game_dir, manifest)?;
        let invalid: Vec<String> = manifest
            .active_patches
            .iter()
            .filter(|id| {
                find_published_patch(game_dir, id)
                    .map(|patch| patch_invalid_for_package(&patch, &package))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        if invalid.is_empty() {
            return Ok(());
        }
        for id in invalid {
            if manifest.active_patches.iter().any(|p| p == &id) {
                manifest.active_patches.retain(|p| p != &id);
                cascaded.push(id);
            }
        }
    }
}

fn reactivate_patch_inner(game_dir: &Path, patch_id: &str) -> EngineResult<ReactivateResult> {
    let mut manifest = read_manifest(game_dir)?;
    ensure_base_package(game_dir)?;
    if manifest.active_patches.iter().any(|p| p == patch_id) {
        return Err(EngineError::PatchRejected(format!(
            "patch '{patch_id}' is already active"
        )));
    }
    find_published_patch(game_dir, patch_id).map_err(|_| {
        EngineError::PatchRejected(format!(
            "no published record for patch '{patch_id}' (cannot re-apply)"
        ))
    })?;
    manifest.active_patches.push(patch_id.to_string());
    sort_active_patches(game_dir, &mut manifest.active_patches);
    refold_canonical(game_dir, &manifest)?;
    write_manifest(game_dir, &manifest)?;
    Ok(ReactivateResult {
        patch_id: patch_id.to_string(),
        active_patches: manifest.active_patches.clone(),
    })
}

fn delete_inactive_patch_inner(game_dir: &Path, patch_id: &str) -> EngineResult<DeletePatchResult> {
    let mut manifest = read_manifest(game_dir)?;
    if manifest.active_patches.iter().any(|p| p == patch_id) {
        return Err(EngineError::PatchRejected(format!(
            "patch '{patch_id}' is still active; roll it back before deleting"
        )));
    }
    if find_published_patch(game_dir, patch_id).is_err() {
        return Err(EngineError::PatchRejected(format!(
            "no published record for patch '{patch_id}'"
        )));
    }
    remove_published_files_for_patch(game_dir, patch_id)?;
    remove_candidate_files_for_patch(game_dir, patch_id)?;
    manifest.patch_labels.remove(patch_id);
    write_manifest(game_dir, &manifest)?;
    let package = load_package(game_dir.join("canonical/package.json"))?;
    let assets_removed = prune_orphan_assets_in_dir(&game_dir.join("assets"), &package);
    Ok(DeletePatchResult {
        patch_id: patch_id.to_string(),
        assets_removed,
    })
}

fn sort_active_patches(game_dir: &Path, active: &mut Vec<String>) {
    active.sort_by_key(|id| published_sequence_for_patch(game_dir, id).unwrap_or(0));
}

fn published_sequence_for_patch(game_dir: &Path, patch_id: &str) -> Option<u64> {
    find_published_record(game_dir, patch_id).map(|(seq, _)| seq)
}

fn find_published_record(game_dir: &Path, patch_id: &str) -> Option<(u64, PatchDocument)> {
    let dir = game_dir.join("published");
    if !dir.is_dir() {
        return None;
    }
    let suffix = format!("_{patch_id}.json");
    let mut best: Option<(u64, PatchDocument)> = None;
    for entry in fs::read_dir(&dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(&suffix) {
            continue;
        }
        let published: PublishedPatch = read_json(entry.path()).ok()?;
        let seq = published.sequence;
        if best.as_ref().is_none_or(|(s, _)| seq > *s) {
            best = Some((seq, published.patch));
        }
    }
    best
}

fn remove_published_files_for_patch(game_dir: &Path, patch_id: &str) -> EngineResult<()> {
    let dir = game_dir.join("published");
    if !dir.is_dir() {
        return Ok(());
    }
    let suffix = format!("_{patch_id}.json");
    for entry in fs::read_dir(&dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(&suffix) {
            fs::remove_file(entry.path()).map_err(io_err)?;
        }
    }
    Ok(())
}

fn remove_candidate_files_for_patch(game_dir: &Path, patch_id: &str) -> EngineResult<()> {
    let name = format!("{patch_id}.json");
    for sub in ["candidates", "candidates/archived"] {
        let path = game_dir.join(sub).join(&name);
        if path.is_file() {
            fs::remove_file(&path).map_err(io_err)?;
        }
    }
    Ok(())
}

fn update_published_labels(
    game_dir: &Path,
    patch_id: &str,
    label: Option<&String>,
) -> EngineResult<()> {
    let dir = game_dir.join("published");
    if !dir.is_dir() {
        return Ok(());
    }
    let suffix = format!("_{patch_id}.json");
    for entry in fs::read_dir(&dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(&suffix) {
            continue;
        }
        let mut published: PublishedPatch = read_json(entry.path())?;
        published.label = label.cloned();
        write_json(entry.path(), &published)?;
    }
    Ok(())
}

fn refold_canonical(game_dir: &Path, manifest: &Manifest) -> EngineResult<GamePackage> {
    let mut package = load_package(game_dir.join("canonical/base.json"))?;
    for patch_id in &manifest.active_patches {
        let patch = find_published_patch(game_dir, patch_id)?;
        patch.validate_version(manifest.package_version)?;
        apply_patch_to_package(&mut package, &patch)?;
    }
    prune_orphan_tile_layers(&mut package);
    validate_package(&package)?;
    write_canonical(game_dir, &package)?;
    Ok(package)
}

fn find_published_patch(game_dir: &Path, patch_id: &str) -> EngineResult<PatchDocument> {
    find_published_record(game_dir, patch_id)
        .map(|(_, p)| p)
        .ok_or_else(|| {
            EngineError::InvalidPackage(format!(
                "no published record for patch '{patch_id}'"
            ))
        })
}

fn list_published_patch_ids_in_order(game_dir: &Path) -> EngineResult<Vec<String>> {
    let dir = game_dir.join("published");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut rows: Vec<(u64, String)> = Vec::new();
    for entry in fs::read_dir(&dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let published: PublishedPatch = read_json(entry.path())?;
        rows.push((published.sequence, published.patch.patch_id));
    }
    rows.sort_by_key(|(seq, _)| *seq);
    Ok(rows.into_iter().map(|(_, id)| id).collect())
}

fn ensure_base_package(game_dir: &Path) -> EngineResult<()> {
    let base_path = game_dir.join("canonical/base.json");
    if base_path.is_file() {
        return Ok(());
    }
    let snap0 = game_dir.join("canonical/snapshots/000000.json");
    if snap0.is_file() {
        let package = load_package(&snap0)?;
        write_base_package(game_dir, &package)?;
        return Ok(());
    }
    let canonical_path = game_dir.join("canonical/package.json");
    if canonical_path.is_file() {
        let package = load_package(&canonical_path)?;
        write_base_package(game_dir, &package)?;
    }
    Ok(())
}

fn write_base_package(game_dir: &Path, package: &GamePackage) -> EngineResult<()> {
    let path = game_dir.join("canonical/base.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    let json = serde_json::to_string_pretty(package)
        .map_err(|e| EngineError::InvalidPackage(e.to_string()))?;
    fs::write(path, json).map_err(io_err)
}

fn max_published_sequence(game_dir: &Path) -> u64 {
    let dir = game_dir.join("published");
    if !dir.exists() {
        return 0;
    }
    let Ok(entries) = fs::read_dir(&dir) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.path()
                .file_name()?
                .to_str()?
                .get(..6)?
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap_or(0)
}

fn write_canonical(game_dir: &Path, package: &GamePackage) -> EngineResult<()> {
    let path = game_dir.join("canonical/package.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    let json = serde_json::to_string_pretty(package)
        .map_err(|e| EngineError::InvalidPackage(e.to_string()))?;
    fs::write(path, json).map_err(io_err)
}

fn status_matches(have: &CandidateStatus, want: &CandidateStatus) -> bool {
    matches!(
        (have, want),
        (CandidateStatus::Pending, CandidateStatus::Pending)
            | (CandidateStatus::Published, CandidateStatus::Published)
            | (CandidateStatus::Rejected, CandidateStatus::Rejected)
    )
}

fn read_manifest(game_dir: &Path) -> EngineResult<Manifest> {
    read_json(game_dir.join("manifest.json"))
}

fn write_manifest(game_dir: &Path, manifest: &Manifest) -> EngineResult<()> {
    write_json(game_dir.join("manifest.json"), manifest)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: impl AsRef<Path>) -> EngineResult<T> {
    let text = fs::read_to_string(path.as_ref()).map_err(io_err)?;
    serde_json::from_str(&text).map_err(|e| EngineError::InvalidPackage(e.to_string()))
}

fn write_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> EngineResult<()> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    let text =
        serde_json::to_string_pretty(value).map_err(|e| EngineError::InvalidPackage(e.to_string()))?;
    fs::write(path.as_ref(), text).map_err(io_err)
}

fn io_err(e: std::io::Error) -> EngineError {
    EngineError::InvalidPackage(format!("io error: {e}"))
}

fn prune_orphan_assets_in_dir(assets_dir: &Path, package: &GamePackage) -> Vec<String> {
    let keep = referenced_asset_ids(package);
    if !assets_dir.is_dir() {
        return Vec::new();
    }
    let mut removed = Vec::new();
    let Ok(entries) = fs::read_dir(assets_dir) else {
        return removed;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !keep.contains(stem) {
            let _ = fs::remove_file(&path);
            removed.push(stem.to_string());
        }
    }
    removed.sort();
    removed
}

/// `skip_subdirs`: when true, only delete `.json` in the top directory (pending inbox).
fn remove_json_files_in(dir: &Path, skip_subdirs: bool) -> EngineResult<u64> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut count = 0u64;
    for entry in fs::read_dir(dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let path = entry.path();
        if path.is_dir() {
            if skip_subdirs {
                continue;
            }
            count += remove_json_files_in(&path, false)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            fs::remove_file(&path).map_err(io_err)?;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_patch::{load_patch, PatchDocument};

    #[test]
    fn seed_submit_publish_flow() {
        let root = std::env::temp_dir().join(format!("aether_plane_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = PlaneStore::new(&root);
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        store
            .seed_package("minimal_explorer", &package)
            .expect("seed");

        let patch_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/patches/add_second_shard.json");
        let patch = load_patch(&patch_path).expect("patch");
        store
            .submit_candidate("minimal_explorer", patch.clone())
            .expect("submit");
        let published = store
            .publish_candidate("minimal_explorer", &patch.patch_id)
            .expect("publish");
        assert_eq!(published.sequence, 1);

        let room_patch_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/patches/add_north_room.json");
        let room_patch = load_patch(&room_patch_path).expect("room patch");
        store
            .submit_candidate("minimal_explorer", room_patch.clone())
            .expect("submit room");
        store
            .publish_candidate("minimal_explorer", &room_patch.patch_id)
            .expect("publish room");

        let canonical = store
            .get_canonical_package("minimal_explorer")
            .expect("canonical");
        assert!(canonical.rooms.iter().any(|r| r.id == "north_grove"));

        store.rollback_last("minimal_explorer").expect("rollback");
        let manifest_after = store.get_manifest("minimal_explorer").expect("manifest");
        assert_eq!(
            manifest_after.published_sequence, 2,
            "published_sequence stays monotonic after rollback"
        );
        assert_eq!(
            manifest_after.active_patches,
            vec!["add_second_shard".to_string()],
            "rollback removes last active patch only"
        );
        let canonical = store
            .get_canonical_package("minimal_explorer")
            .expect("canonical after rollback");
        assert!(
            !canonical.rooms.iter().any(|r| r.id == "north_grove"),
            "rollback should remove merged room"
        );

        store
            .submit_candidate("minimal_explorer", room_patch.clone())
            .expect("submit room again");
        let republished = store
            .publish_candidate("minimal_explorer", &room_patch.patch_id)
            .expect("republish");
        assert_eq!(
            republished.sequence, 3,
            "republish after rollback gets a new audit sequence"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn consecutive_rollbacks_step_back_each_time() {
        let root = std::env::temp_dir().join(format!("aether_rb2_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = PlaneStore::new(&root);
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        store
            .seed_package("minimal_explorer", &package)
            .expect("seed");

        let shard = load_patch(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/minimal_explorer/patches/add_second_shard.json"),
        )
        .expect("shard");
        let room = load_patch(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/minimal_explorer/patches/add_north_room.json"),
        )
        .expect("room");

        store
            .submit_candidate("minimal_explorer", shard.clone())
            .expect("submit");
        store
            .publish_candidate("minimal_explorer", &shard.patch_id)
            .expect("pub1");
        store
            .submit_candidate("minimal_explorer", room.clone())
            .expect("submit");
        store
            .publish_candidate("minimal_explorer", &room.patch_id)
            .expect("pub2");
        assert_eq!(store.get_manifest("minimal_explorer").unwrap().published_sequence, 2);
        assert_eq!(
            store.get_manifest("minimal_explorer").unwrap().active_patches.len(),
            2
        );

        store.rollback_last("minimal_explorer").expect("rb1");
        let m1 = store.get_manifest("minimal_explorer").unwrap();
        assert_eq!(m1.published_sequence, 2);
        assert_eq!(m1.active_patches, vec!["add_second_shard".to_string()]);
        let after_rb1 = store.get_canonical_package("minimal_explorer").unwrap();
        assert!(!after_rb1.rooms.iter().any(|r| r.id == "north_grove"));

        store.rollback_last("minimal_explorer").expect("rb2");
        let m2 = store.get_manifest("minimal_explorer").unwrap();
        assert_eq!(m2.published_sequence, 2);
        assert!(m2.active_patches.is_empty());
        let after_rb2 = store.get_canonical_package("minimal_explorer").unwrap();
        assert!(!after_rb2
            .entities
            .iter()
            .any(|e| e.id == "grove_shard_north"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rollback_patch_id_out_of_order() {
        let root = std::env::temp_dir().join(format!("aether_rb_ood_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = PlaneStore::new(&root);
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        store
            .seed_package("minimal_explorer", &package)
            .expect("seed");

        let shard = load_patch(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/minimal_explorer/patches/add_second_shard.json"),
        )
        .expect("shard");
        let room = load_patch(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/minimal_explorer/patches/add_north_room.json"),
        )
        .expect("room");

        store
            .submit_candidate("minimal_explorer", shard.clone())
            .expect("submit");
        store
            .publish_candidate("minimal_explorer", &shard.patch_id)
            .expect("pub1");
        store
            .submit_candidate("minimal_explorer", room.clone())
            .expect("submit");
        store
            .publish_candidate("minimal_explorer", &room.patch_id)
            .expect("pub2");

        store
            .rollback_patch("minimal_explorer", Some("add_second_shard"))
            .expect("rb first patch while room stays");

        let m = store.get_manifest("minimal_explorer").unwrap();
        assert_eq!(m.active_patches, vec!["add_north_room".to_string()]);
        let pkg = store.get_canonical_package("minimal_explorer").unwrap();
        assert!(pkg.rooms.iter().any(|r| r.id == "north_grove"));
        assert!(!pkg.entities.iter().any(|e| e.id == "grove_shard_north"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_orphan_assets_keeps_canonical_references() {
        let root = std::env::temp_dir().join(format!("aether_prune_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = PlaneStore::new(&root);
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        store
            .seed_package("minimal_explorer", &package)
            .expect("seed");

        let assets = store.assets_dir("minimal_explorer");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("art_keep.png"), b"x").unwrap();
        fs::write(assets.join("art_orphan.png"), b"x").unwrap();

        let mut pkg = store.get_canonical_package("minimal_explorer").unwrap();
        pkg.entities.push(aether_package::EntityDef {
            id: "prop".into(),
            scene: "main".into(),
            components: aether_package::EntityComponents {
                sprite: Some(aether_package::SpriteDef {
                    asset: "art_keep".into(),
                }),
                ..Default::default()
            },
        });
        let game_dir = store.game_dir("minimal_explorer");
        write_canonical(&game_dir, &pkg).unwrap();

        let removed = store.prune_orphan_assets("minimal_explorer").expect("prune");
        assert!(removed.contains(&"art_orphan".to_string()));
        assert!(!removed.contains(&"art_keep".to_string()));
        assert!(!assets.join("art_keep.png").exists() || assets.join("art_keep.png").is_file());
        assert!(!assets.join("art_orphan.png").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rollback_room_cascades_paint_tiles() {
        let root = std::env::temp_dir().join(format!("aether_cascade_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = PlaneStore::new(&root);
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        store
            .seed_package("minimal_explorer", &package)
            .expect("seed");

        let room = load_patch(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/minimal_explorer/patches/add_north_room.json"),
        )
        .expect("room");
        let paint = PatchDocument {
            patch_id: "paint_north_moss".into(),
            base_version: 1,
            ops: vec![serde_json::json!({
                "op": "paint_tiles",
                "scene": "main",
                "room_id": "north_grove",
                "tile_size": 16.0,
                "fill": "moss"
            })],
        };

        store
            .submit_candidate("minimal_explorer", room.clone())
            .expect("submit room");
        store
            .publish_candidate("minimal_explorer", &room.patch_id)
            .expect("pub room");
        store
            .submit_candidate("minimal_explorer", paint.clone())
            .expect("submit paint");
        store
            .publish_candidate("minimal_explorer", &paint.patch_id)
            .expect("pub paint");

        let before = store.get_canonical_package("minimal_explorer").unwrap();
        assert!(before.tile_layers.iter().any(|l| l.room_id == "north_grove"));

        let rolled = store
            .rollback_patch("minimal_explorer", Some("add_north_room"))
            .expect("rollback room");
        assert_eq!(rolled.removed_patch_id, "add_north_room");
        assert!(
            rolled.cascaded_patch_ids.contains(&"paint_north_moss".to_string()),
            "moss patch should cascade off with the room"
        );

        let after = store.get_canonical_package("minimal_explorer").unwrap();
        assert!(!after.rooms.iter().any(|r| r.id == "north_grove"));
        assert!(!after.tile_layers.iter().any(|l| l.room_id == "north_grove"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reactivate_and_delete_inactive_patch() {
        let root = std::env::temp_dir().join(format!("aether_react_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = PlaneStore::new(&root);
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        store
            .seed_package("minimal_explorer", &package)
            .expect("seed");

        let room = load_patch(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/minimal_explorer/patches/add_north_room.json"),
        )
        .expect("room");
        store
            .submit_candidate("minimal_explorer", room.clone())
            .expect("submit");
        store
            .publish_candidate("minimal_explorer", &room.patch_id)
            .expect("publish");
        store.rollback_last("minimal_explorer").expect("rollback");

        store
            .set_patch_label("minimal_explorer", &room.patch_id, "North grove v1")
            .expect("label");
        let manifest = store.get_manifest("minimal_explorer").unwrap();
        assert_eq!(
            manifest.patch_labels.get(&room.patch_id).map(String::as_str),
            Some("North grove v1")
        );

        store
            .reactivate_patch("minimal_explorer", &room.patch_id)
            .expect("reactivate");
        assert!(store
            .get_canonical_package("minimal_explorer")
            .unwrap()
            .rooms
            .iter()
            .any(|r| r.id == "north_grove"));

        store.rollback_last("minimal_explorer").expect("rollback again");
        store
            .delete_inactive_patch("minimal_explorer", &room.patch_id)
            .expect("delete");
        assert!(store
            .reactivate_patch("minimal_explorer", &room.patch_id)
            .is_err());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn save_and_list_generation_traces() {
        use crate::generation::GenerationTrace;

        let root = std::env::temp_dir().join(format!("aether_trace_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = PlaneStore::new(&root);
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        store
            .seed_package("minimal_explorer", &package)
            .expect("seed");

        let trace = GenerationTrace::new(
            "simulated",
            None,
            "add a room".into(),
            r#"{"ops":[]}"#.into(),
        );
        let path = store
            .save_generation_trace(
                "minimal_explorer",
                "add a room",
                "simulated",
                "patch_test_1",
                &trace,
            )
            .expect("save trace");
        assert!(path.is_file());

        let listed = store
            .list_generation_traces("minimal_explorer")
            .expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].patch_id, "patch_test_1");
        assert_eq!(listed[0].prompt, "add a room");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn submit_rejects_noop_patch() {
        let root = std::env::temp_dir().join(format!("aether_noop_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = PlaneStore::new(&root);
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        store
            .seed_package("minimal_explorer", &package)
            .expect("seed");

        let canonical = store.get_canonical_package("minimal_explorer").unwrap();
        let room = canonical
            .rooms
            .iter()
            .find(|r| r.id == "starting_grove")
            .expect("room")
            .clone();
        let noop = PatchDocument {
            patch_id: "noop_test".into(),
            base_version: 1,
            ops: vec![serde_json::json!({
                "op": "upsert_room",
                "scene": "main",
                "room": room,
            })],
        };
        let err = store
            .submit_candidate("minimal_explorer", noop)
            .expect_err("noop");
        assert!(err.to_string().contains("no-op"));

        let _ = fs::remove_dir_all(&root);
    }
}
