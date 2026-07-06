//! HTTP + SSE API for the content plane.

use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;

use aether_patch::PatchDocument;
use async_stream::stream;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use futures::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::director::{patch_intent_from_event, GameplayEvent};
use crate::generation::{
    self, prompts, GenerateArtRequest, GenerateArtResponse, GenerateGameRequest,
    GenerateGameResponse, GenerateMode, GenerateRequest, GenerateResponse, GenerateWorldRequest,
    GenerateWorldResponse,
};
use crate::store::{
    CandidateRecord, CandidateStatus, DeletePatchResult, Manifest, PlaneStore, PruneReport,
    PruneRequest, PublishedPatch, RollbackResult,
};

pub type PatchBus = broadcast::Sender<StreamEvent>;

#[derive(Clone)]
pub struct AppState {
    pub store: PlaneStore,
    pub bus: PatchBus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    PatchPublished {
        game_id: String,
        sequence: u64,
        patch: PatchDocument,
    },
    /// Canonical package was restored (rollback); runtimes should reload from GET /canonical.
    CanonicalReset {
        game_id: String,
        reason: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct SinceQuery {
    #[serde(default)]
    pub since: u64,
}

#[derive(Debug, Deserialize)]
pub struct CandidateListQuery {
    /// When set, only return candidates with this status (`pending`, `published`, `rejected`).
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

pub fn router(state: AppState, studio_dir: PathBuf) -> Router {
    let studio_index = studio_dir.join("index.html");
    Router::new()
        .route("/health", get(health))
        .route("/health/ollama", get(health_ollama))
        .route("/health/ollama/activity", get(health_ollama_activity))
        .route("/packages/{game_id}/manifest", get(get_manifest))
        .route("/packages/{game_id}/canonical", get(get_canonical))
        .route("/packages/{game_id}/patches", get(list_patches))
        .route("/packages/{game_id}/candidates", get(list_candidates).post(submit_candidate))
        .route(
            "/packages/{game_id}/candidates/{patch_id}/publish",
            post(publish_candidate),
        )
        .route(
            "/packages/{game_id}/candidates/{patch_id}/reject",
            post(reject_candidate),
        )
        .route("/packages/{game_id}/stream", get(patch_stream))
        .route("/packages/{game_id}/events", post(ingest_event))
        .route("/packages/{game_id}/generate", post(generate_from_prompt))
        .route("/packages/{game_id}/generate-art", post(generate_art_handler))
        .route("/packages/{game_id}/generate-world", post(generate_world_handler))
        .route("/packages/{game_id}/generate-game", post(generate_game_handler))
        .route("/packages/{game_id}/traces", get(list_generation_traces_handler))
        .route("/packages/{game_id}/rollback", post(rollback_publish))
        .route(
            "/packages/{game_id}/patches/{patch_id}/label",
            patch(set_patch_label),
        )
        .route(
            "/packages/{game_id}/patches/{patch_id}/reactivate",
            post(reactivate_patch_handler),
        )
        .route(
            "/packages/{game_id}/patches/{patch_id}",
            delete(delete_inactive_patch_handler),
        )
        .route("/packages/{game_id}/prune", post(prune_workspace))
        .route("/packages/{game_id}/assets/{asset_id}", get(get_asset))
        .nest_service(
            "/studio",
            ServeDir::new(&studio_dir).not_found_service(ServeFile::new(studio_index)),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn health_ollama_activity() -> Json<generation::OllamaActivityReport> {
    let report = tokio::task::spawn_blocking(generation::ollama_activity)
        .await
        .unwrap_or_else(|e| generation::OllamaActivityReport {
            state: "unreachable".into(),
            reachable: false,
            configured_model: String::new(),
            running_models: vec![],
            message: format!("activity check failed: {e}"),
        });
    Json(report)
}

async fn health_ollama() -> Json<generation::OllamaHealthReport> {
    let report = tokio::task::spawn_blocking(generation::check_ollama_health)
        .await
        .unwrap_or_else(|e| generation::OllamaHealthReport {
            ok: false,
            url: String::new(),
            configured_model: String::new(),
            model_available: false,
            models: vec![],
            timeout_secs: 0,
            message: format!("health check task failed: {e}"),
        });
    Json(report)
}

async fn get_manifest(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> Result<Json<Manifest>, (StatusCode, Json<ErrorBody>)> {
    state
        .store
        .get_manifest(&game_id)
        .map(Json)
        .map_err(map_err)
}

async fn get_canonical(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorBody>)> {
    state
        .store
        .get_canonical_package(&game_id)
        .map(|p| Json(serde_json::to_value(p).unwrap()))
        .map_err(map_err)
}

async fn list_patches(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Query(query): Query<SinceQuery>,
) -> Result<Json<Vec<PublishedPatch>>, (StatusCode, Json<ErrorBody>)> {
    let manifest = state.store.get_manifest(&game_id).map_err(map_err)?;
    let mut patches = state
        .store
        .list_published_since(&game_id, query.since)
        .map_err(map_err)?;
    for published in &mut patches {
        if published.label.is_none() {
            published.label = manifest.patch_labels.get(&published.patch.patch_id).cloned();
        }
    }
    Ok(Json(patches))
}

async fn list_candidates(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Query(query): Query<CandidateListQuery>,
) -> Result<Json<Vec<CandidateRecord>>, (StatusCode, Json<ErrorBody>)> {
    let status = query
        .status
        .as_deref()
        .map(parse_candidate_status)
        .transpose()
        .map_err(|msg| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody { error: msg }),
            )
        })?;
    state
        .store
        .list_candidates(&game_id, status)
        .map(Json)
        .map_err(map_err)
}

fn parse_candidate_status(s: &str) -> Result<CandidateStatus, String> {
    match s {
        "pending" => Ok(CandidateStatus::Pending),
        "published" => Ok(CandidateStatus::Published),
        "rejected" => Ok(CandidateStatus::Rejected),
        _ => Err(format!(
            "invalid status '{s}' (use pending, published, or rejected)"
        )),
    }
}

async fn submit_candidate(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(patch): Json<PatchDocument>,
) -> Result<StatusCode, (StatusCode, Json<ErrorBody>)> {
    state.store.submit_candidate(&game_id, patch).map_err(map_err)?;
    Ok(StatusCode::CREATED)
}

async fn publish_candidate(
    State(state): State<AppState>,
    Path((game_id, patch_id)): Path<(String, String)>,
) -> Result<Json<PublishedPatch>, (StatusCode, Json<ErrorBody>)> {
    let published = state
        .store
        .publish_candidate(&game_id, &patch_id)
        .map_err(map_err)?;
    let _ = state.bus.send(StreamEvent::PatchPublished {
        game_id: game_id.clone(),
        sequence: published.sequence,
        patch: published.patch.clone(),
    });
    Ok(Json(published))
}

async fn reject_candidate(
    State(state): State<AppState>,
    Path((game_id, patch_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorBody>)> {
    state
        .store
        .reject_candidate(&game_id, &patch_id)
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn generate_from_prompt(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(req): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, (StatusCode, Json<ErrorBody>)> {
    let manifest = state.store.get_manifest(&game_id).map_err(map_err)?;
    let canonical = state.store.get_canonical_package(&game_id).map_err(map_err)?;
    let pending = state
        .store
        .list_candidates(&game_id, Some(crate::store::CandidateStatus::Pending))
        .map_err(map_err)?;
    let pending_patches: Vec<_> = pending.iter().map(|c| c.patch.clone()).collect();
    let scene_id = canonical
        .default_scene_id()
        .map_err(map_err)?
        .to_string();
    let summary = prompts::canonical_summary(&canonical);

    let prompt = req.prompt;
    let prompt_for_trace = prompt.clone();
    let mode = req.mode;
    let package_version = manifest.package_version;
    let game_id_blocking = game_id.clone();

    let outcome = tokio::task::spawn_blocking(move || {
        generation::generate_patch(
            &prompt,
            mode,
            package_version,
            &scene_id,
            &game_id_blocking,
            Some(summary.as_str()),
            &canonical,
            &pending_patches,
        )
    })
    .await
    .map_err(blocking_join_err)?
    .map_err(map_err)?;

    let provider = match req.mode {
        GenerateMode::Simulated => "simulated",
        GenerateMode::Ollama => {
            if generation::ollama_use_legacy_patch() {
                "ollama"
            } else {
                "ollama_plan"
            }
        }
        GenerateMode::Dream => "ollama_dream",
        GenerateMode::OpenAi => "openai",
    };

    if let Some(ref trace) = outcome.trace {
        let patch_id = outcome.patch.patch_id.clone();
        let _ = state.store.save_generation_trace(
            &game_id,
            &prompt_for_trace,
            provider,
            &patch_id,
            trace,
        );
    }

    let mut submitted = false;
    if req.auto_submit {
        state
            .store
            .submit_candidate(&game_id, outcome.patch.clone())
            .map_err(map_err)?;
        submitted = true;
    }

    Ok(Json(GenerateResponse {
        patch: outcome.patch,
        provider,
        submitted,
        trace: outcome.trace,
    }))
}

async fn generate_world_handler(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(req): Json<GenerateWorldRequest>,
) -> Result<Json<GenerateWorldResponse>, (StatusCode, Json<ErrorBody>)> {
    let manifest = state.store.get_manifest(&game_id).map_err(map_err)?;
    let assets_dir = state.store.assets_dir(&game_id);
    let canonical = state.store.get_canonical_package(&game_id).map_err(map_err)?;
    let scene_id = canonical
        .default_scene_id()
        .map_err(map_err)?
        .to_string();

    let prompt = req.prompt;
    let prompt_for_trace = prompt.clone();
    let patch_mode = req.patch_mode;
    let art_mode = req.art_mode;
    let placement = req.placement;
    let include_art = req.include_art;
    let package_version = manifest.package_version;
    let game_id_blocking = game_id.clone();

    let mut response = tokio::task::spawn_blocking(move || {
        generation::generate_world(
            &game_id_blocking,
            &prompt,
            patch_mode,
            art_mode,
            placement,
            include_art,
            &scene_id,
            package_version,
            &assets_dir,
            &canonical,
        )
    })
    .await
    .map_err(blocking_join_err)?
    .map_err(map_err)?;

    if let Some(ref trace) = response.trace {
        let patch_id = response.patch.patch_id.clone();
        let _ = state.store.save_generation_trace(
            &game_id,
            &prompt_for_trace,
            response.patch_provider,
            &patch_id,
            trace,
        );
    }

    if req.auto_submit {
        state
            .store
            .submit_candidate(&game_id, response.patch.clone())
            .map_err(map_err)?;
        response.submitted = true;
    }

    Ok(Json(response))
}

async fn generate_game_handler(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(req): Json<GenerateGameRequest>,
) -> Result<Json<GenerateGameResponse>, (StatusCode, Json<ErrorBody>)> {
    let assets_dir = state.store.assets_dir(&game_id);
    let prompt = req.prompt;
    let patch_mode = req.patch_mode;
    let art_mode = req.art_mode;
    let placement = req.placement;
    let include_art = req.include_art;
    let apply = req.apply_canonical;
    let game_id_blocking = game_id.clone();

    let mut response = tokio::task::spawn_blocking(move || {
        generation::generate_game(
            &game_id_blocking,
            &prompt,
            patch_mode,
            art_mode,
            placement,
            include_art,
            &assets_dir,
        )
    })
    .await
    .map_err(blocking_join_err)?
    .map_err(map_err)?;

    if apply {
        let package = response.package.clone();
        let patch = response.patch.clone();
        state
            .store
            .replace_canonical_package(&game_id, package, patch)
            .map_err(map_err)?;
        let _ = state.bus.send(StreamEvent::CanonicalReset {
            game_id: game_id.clone(),
            reason: "generate_game".into(),
        });
        response.applied = true;
    }

    Ok(Json(response))
}

async fn generate_art_handler(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(req): Json<GenerateArtRequest>,
) -> Result<Json<GenerateArtResponse>, (StatusCode, Json<ErrorBody>)> {
    let manifest = state.store.get_manifest(&game_id).map_err(map_err)?;
    let assets_dir = state.store.assets_dir(&game_id);
    let prompt = req.prompt;
    let mode = req.mode;
    let placement = req.placement;
    let entity_id = req.entity_id;
    let scene = req.scene;
    let package_version = manifest.package_version;

    let (asset_id, patch, provider, placement_provider) = tokio::task::spawn_blocking(move || {
        generation::generate_art(
            &prompt,
            mode,
            placement,
            &entity_id,
            &scene,
            package_version,
            &assets_dir,
        )
    })
    .await
    .map_err(blocking_join_err)?
    .map_err(map_err)?;
    let mut submitted = false;
    if req.auto_submit {
        state
            .store
            .submit_candidate(&game_id, patch.clone())
            .map_err(map_err)?;
        submitted = true;
    }
    Ok(Json(GenerateArtResponse {
        asset_id,
        patch,
        provider,
        placement: placement_provider,
        submitted,
    }))
}

async fn prune_workspace(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(req): Json<PruneRequest>,
) -> Result<Json<PruneReport>, (StatusCode, Json<ErrorBody>)> {
    let report = state.store.prune_workspace(&game_id, &req).map_err(map_err)?;
    Ok(Json(report))
}

#[derive(Debug, Deserialize, Default)]
struct RollbackRequest {
    patch_id: Option<String>,
}

async fn rollback_publish(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    body: Option<Json<RollbackRequest>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorBody>)> {
    let patch_id = body.map(|b| b.patch_id.clone()).unwrap_or_default();
    let rolled = state
        .store
        .rollback_patch(&game_id, patch_id.as_deref())
        .map_err(map_err)?;
    let manifest = state.store.get_manifest(&game_id).map_err(map_err)?;
    let _ = state.bus.send(StreamEvent::CanonicalReset {
        game_id: game_id.clone(),
        reason: "rollback".into(),
    });
    Ok(rollback_response(&rolled, &manifest))
}

fn rollback_response(rolled: &RollbackResult, manifest: &Manifest) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "removed_patch_id": rolled.removed_patch_id,
        "cascaded_patch_ids": rolled.cascaded_patch_ids,
        "active_patches": rolled.active_patches,
        "published_sequence": manifest.published_sequence,
        "message": rollback_message(rolled),
    }))
}

fn rollback_message(rolled: &RollbackResult) -> String {
    if rolled.cascaded_patch_ids.is_empty() {
        format!(
            "Rolled back '{}'; {} active patch(es) remain (players reload via SSE)",
            rolled.removed_patch_id,
            rolled.active_patches.len()
        )
    } else {
        format!(
            "Rolled back '{}' and cascaded {:?}; {} active patch(es) remain",
            rolled.removed_patch_id,
            rolled.cascaded_patch_ids,
            rolled.active_patches.len()
        )
    }
}

#[derive(Debug, Deserialize)]
struct PatchLabelBody {
    label: String,
}

async fn set_patch_label(
    State(state): State<AppState>,
    Path((game_id, patch_id)): Path<(String, String)>,
    Json(body): Json<PatchLabelBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorBody>)> {
    state
        .store
        .set_patch_label(&game_id, &patch_id, &body.label)
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({
        "patch_id": patch_id,
        "label": body.label.trim(),
    })))
}

async fn reactivate_patch_handler(
    State(state): State<AppState>,
    Path((game_id, patch_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorBody>)> {
    let result = state
        .store
        .reactivate_patch(&game_id, &patch_id)
        .map_err(map_err)?;
    let manifest = state.store.get_manifest(&game_id).map_err(map_err)?;
    let _ = state.bus.send(StreamEvent::CanonicalReset {
        game_id: game_id.clone(),
        reason: "reactivate".into(),
    });
    Ok(Json(serde_json::json!({
        "patch_id": result.patch_id,
        "active_patches": result.active_patches,
        "published_sequence": manifest.published_sequence,
        "message": format!("Re-applied patch '{}'", result.patch_id),
    })))
}

async fn delete_inactive_patch_handler(
    State(state): State<AppState>,
    Path((game_id, patch_id)): Path<(String, String)>,
) -> Result<Json<DeletePatchResult>, (StatusCode, Json<ErrorBody>)> {
    state
        .store
        .delete_inactive_patch(&game_id, &patch_id)
        .map(Json)
        .map_err(map_err)
}

async fn get_asset(
    State(state): State<AppState>,
    Path((game_id, asset_id)): Path<(String, String)>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ErrorBody>)> {
    let path = state.store.asset_path(&game_id, &asset_id);
    let bytes = std::fs::read(&path).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("asset not found: {e}"),
            }),
        )
    })?;
    Ok((
        [(axum::http::header::CONTENT_TYPE, "image/png")],
        bytes,
    ))
}

async fn ingest_event(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(event): Json<GameplayEvent>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorBody>)> {
    let manifest = state.store.get_manifest(&game_id).map_err(map_err)?;
    let scene_id = state
        .store
        .get_canonical_package(&game_id)
        .ok()
        .and_then(|p| p.default_scene_id().ok().map(|s| s.to_string()))
        .unwrap_or_else(|| "main".to_string());

    let mut created = serde_json::json!({ "accepted": true, "candidate": null });
    if let Some(patch) = patch_intent_from_event(manifest.package_version, &scene_id, &event) {
        let patch_id = patch.patch_id.clone();
        state.store.submit_candidate(&game_id, patch).map_err(map_err)?;
        created["candidate"] = serde_json::json!({ "patch_id": patch_id });
    }
    Ok(Json(created))
}

async fn patch_stream(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Query(query): Query<SinceQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorBody>)> {
    let history = state
        .store
        .list_published_since(&game_id, query.since)
        .map_err(map_err)?;

    let bus_rx = state.bus.subscribe();
    let game_id_for_live = game_id.clone();
    let since = query.since;

    let stream = stream! {
        let mut last_seen = since;
        for published in history {
            last_seen = last_seen.max(published.sequence);
            let payload = serde_json::to_string(&StreamEvent::PatchPublished {
                game_id: game_id.clone(),
                sequence: published.sequence,
                patch: published.patch,
            }).unwrap();
            yield Ok(Event::default().id(published.sequence.to_string()).data(payload));
        }

        let mut live = BroadcastStream::new(bus_rx);
        while let Some(Ok(event)) = live.next().await {
            match &event {
                StreamEvent::PatchPublished { game_id: gid, sequence, .. } => {
                    if gid != &game_id_for_live || *sequence <= last_seen {
                        continue;
                    }
                    last_seen = *sequence;
                    let payload = serde_json::to_string(&event).unwrap();
                    yield Ok(Event::default().id(sequence.to_string()).data(payload));
                }
                StreamEvent::CanonicalReset { game_id: gid, .. } => {
                    if gid != &game_id_for_live {
                        continue;
                    }
                    let payload = serde_json::to_string(&event).unwrap();
                    yield Ok(Event::default().id("canonical_reset").data(payload));
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

#[derive(Debug, Serialize)]
struct TraceListResponse {
    traces: Vec<generation::PersistedGenerationTrace>,
}

async fn list_generation_traces_handler(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> Result<Json<TraceListResponse>, (StatusCode, Json<ErrorBody>)> {
    let traces = state
        .store
        .list_generation_traces(&game_id)
        .map_err(map_err)?;
    Ok(Json(TraceListResponse { traces }))
}

fn blocking_join_err(err: tokio::task::JoinError) -> (StatusCode, Json<ErrorBody>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody {
            error: format!("background task failed: {err}"),
        }),
    )
}

fn map_err(err: aether_core::EngineError) -> (StatusCode, Json<ErrorBody>) {
    let status = match &err {
        aether_core::EngineError::PackageNotFound(_) => StatusCode::NOT_FOUND,
        aether_core::EngineError::PatchRejected(_) => StatusCode::BAD_REQUEST,
        aether_core::EngineError::InvalidPackage(_) => StatusCode::BAD_REQUEST,
    };
    (
        status,
        Json(ErrorBody {
            error: err.to_string(),
        }),
    )
}
