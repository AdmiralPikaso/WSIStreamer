//! HTTP handlers for slide annotations.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

use crate::annotation::{
    parse_import_payload, AnnotationError, AnnotationFormat, AnnotationQuery,
    BatchAnnotationRequest, CreateAnnotationRequest, FormatQuery, UpdateAnnotationRequest,
};
use crate::server::handlers::{AppState, ErrorResponse};
use crate::slide::SlideSource;

impl IntoResponse for AnnotationError {
    fn into_response(self) -> Response {
        let (status, error, message) = match self {
            AnnotationError::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            AnnotationError::Invalid(message) => {
                (StatusCode::BAD_REQUEST, "invalid_request", message)
            }
            AnnotationError::UnsupportedFormat(message) => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_format",
                message,
            ),
            AnnotationError::Storage(message) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error", message)
            }
        };
        (
            status,
            Json(ErrorResponse::with_status(error, message, status)),
        )
            .into_response()
    }
}

pub async fn list_annotations_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Path(slide_id): Path<String>,
    Query(query): Query<AnnotationQuery>,
) -> Result<impl IntoResponse, AnnotationError> {
    let annotations = state.annotation_store.list(&slide_id, &query).await?;
    Ok(Json(annotations))
}

pub async fn create_annotation_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Path(slide_id): Path<String>,
    Json(request): Json<CreateAnnotationRequest>,
) -> Result<impl IntoResponse, AnnotationError> {
    let annotation = state.annotation_store.create(slide_id, request).await?;
    Ok((StatusCode::CREATED, Json(annotation)))
}

pub async fn get_annotation_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Path(annotation_id): Path<String>,
) -> Result<impl IntoResponse, AnnotationError> {
    let annotation = state.annotation_store.get(&annotation_id).await?;
    Ok(Json(annotation))
}

pub async fn update_annotation_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Path(annotation_id): Path<String>,
    Json(request): Json<UpdateAnnotationRequest>,
) -> Result<impl IntoResponse, AnnotationError> {
    let annotation = state
        .annotation_store
        .update(&annotation_id, request)
        .await?;
    Ok(Json(annotation))
}

pub async fn update_slide_annotation_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Path((_slide_id, annotation_id)): Path<(String, String)>,
    Json(request): Json<UpdateAnnotationRequest>,
) -> Result<impl IntoResponse, AnnotationError> {
    let annotation = state
        .annotation_store
        .update(&annotation_id, request)
        .await?;
    Ok(Json(annotation))
}

pub async fn delete_annotation_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Path(annotation_id): Path<String>,
) -> Result<impl IntoResponse, AnnotationError> {
    state.annotation_store.delete(&annotation_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_slide_annotation_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Path((_slide_id, annotation_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AnnotationError> {
    state.annotation_store.delete(&annotation_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn batch_annotations_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Json(request): Json<BatchAnnotationRequest>,
) -> Result<impl IntoResponse, AnnotationError> {
    let response = state.annotation_store.batch(request).await?;
    Ok(Json(response))
}

pub async fn export_annotations_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Path(slide_id): Path<String>,
    Query(query): Query<FormatQuery>,
) -> Result<Response, AnnotationError> {
    let body = state
        .annotation_store
        .export_slide(&slide_id, query.format)
        .await?;
    let content_type = match query.format {
        AnnotationFormat::Json => "application/json",
        AnnotationFormat::Geojson => "application/geo+json",
        AnnotationFormat::Xml | AnnotationFormat::Aperio => "application/xml",
    };
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(axum::body::Body::from(body))
        .unwrap())
}

#[derive(Debug, serde::Deserialize)]
pub struct ImportQuery {
    #[serde(default = "default_import_format")]
    pub format: AnnotationFormat,
    #[serde(default = "default_author")]
    pub author_id: String,
}

fn default_import_format() -> AnnotationFormat {
    AnnotationFormat::Json
}

fn default_author() -> String {
    "import".to_string()
}

pub async fn import_annotations_handler<S: SlideSource>(
    State(state): State<AppState<S>>,
    Path(slide_id): Path<String>,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<impl IntoResponse, AnnotationError> {
    let requests = parse_import_payload(&slide_id, query.format, &body, query.author_id)?;
    let mut created = Vec::with_capacity(requests.len());
    for request in requests {
        created.push(
            state
                .annotation_store
                .create(slide_id.clone(), request)
                .await?,
        );
    }
    Ok((StatusCode::CREATED, Json(created)))
}
