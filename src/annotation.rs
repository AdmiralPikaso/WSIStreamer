//! Annotation data model, validation, storage, and import/export helpers.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum AnnotationError {
    #[error("annotation not found: {0}")]
    NotFound(String),
    #[error("invalid annotation: {0}")]
    Invalid(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl BoundingBox {
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.x <= other.x + other.width
            && self.x + self.width >= other.x
            && self.y <= other.y + other.height
            && self.y + self.height >= other.y
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationType {
    Point,
    Rectangle,
    Polygon,
    Circle,
    Ellipse,
    Line,
    Polyline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Geometry {
    Point {
        point: Point,
    },
    Rectangle {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Polygon {
        points: Vec<Point>,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    Ellipse {
        center: Point,
        radius_x: f64,
        radius_y: f64,
    },
    Line {
        start: Point,
        end: Point,
    },
    Polyline {
        points: Vec<Point>,
    },
}

impl Geometry {
    pub fn annotation_type(&self) -> AnnotationType {
        match self {
            Geometry::Point { .. } => AnnotationType::Point,
            Geometry::Rectangle { .. } => AnnotationType::Rectangle,
            Geometry::Polygon { .. } => AnnotationType::Polygon,
            Geometry::Circle { .. } => AnnotationType::Circle,
            Geometry::Ellipse { .. } => AnnotationType::Ellipse,
            Geometry::Line { .. } => AnnotationType::Line,
            Geometry::Polyline { .. } => AnnotationType::Polyline,
        }
    }

    pub fn bbox(&self) -> BoundingBox {
        match self {
            Geometry::Point { point } => BoundingBox {
                x: point.x,
                y: point.y,
                width: 0.0,
                height: 0.0,
            },
            Geometry::Rectangle {
                x,
                y,
                width,
                height,
            } => BoundingBox {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
            },
            Geometry::Polygon { points } | Geometry::Polyline { points } => points_bbox(points),
            Geometry::Circle { center, radius } => BoundingBox {
                x: center.x - radius,
                y: center.y - radius,
                width: radius * 2.0,
                height: radius * 2.0,
            },
            Geometry::Ellipse {
                center,
                radius_x,
                radius_y,
            } => BoundingBox {
                x: center.x - radius_x,
                y: center.y - radius_y,
                width: radius_x * 2.0,
                height: radius_y * 2.0,
            },
            Geometry::Line { start, end } => points_bbox(&[*start, *end]),
        }
    }

    pub fn validate(&self) -> Result<(), AnnotationError> {
        match self {
            Geometry::Point { point } => validate_point(point),
            Geometry::Rectangle {
                x,
                y,
                width,
                height,
            } => {
                validate_number("x", *x)?;
                validate_number("y", *y)?;
                validate_positive("width", *width)?;
                validate_positive("height", *height)
            }
            Geometry::Polygon { points } => {
                validate_points(points, 3, "polygon requires at least 3 points")
            }
            Geometry::Circle { center, radius } => {
                validate_point(center)?;
                validate_positive("radius", *radius)
            }
            Geometry::Ellipse {
                center,
                radius_x,
                radius_y,
            } => {
                validate_point(center)?;
                validate_positive("radius_x", *radius_x)?;
                validate_positive("radius_y", *radius_y)
            }
            Geometry::Line { start, end } => {
                validate_point(start)?;
                validate_point(end)
            }
            Geometry::Polyline { points } => {
                validate_points(points, 2, "polyline requires at least 2 points")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationStyle {
    pub color: String,
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_radius: Option<f64>,
}

impl Default for AnnotationStyle {
    fn default() -> Self {
        Self {
            color: "#ff3366".to_string(),
            opacity: 0.55,
            point_radius: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub slide_id: String,
    pub annotation_type: AnnotationType,
    pub geometry: Geometry,
    pub bbox: BoundingBox,
    pub style: AnnotationStyle,
    pub label: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub author_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAnnotationRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub geometry: Geometry,
    #[serde(default)]
    pub style: AnnotationStyle,
    #[serde(default)]
    pub label: Option<String>,
    pub author_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAnnotationRequest {
    #[serde(default)]
    pub geometry: Option<Geometry>,
    #[serde(default)]
    pub style: Option<AnnotationStyle>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnnotationQuery {
    #[serde(default)]
    pub annotation_type: Option<AnnotationType>,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchAnnotationRequest {
    #[serde(default)]
    pub create: Vec<BatchCreateAnnotation>,
    #[serde(default)]
    pub update: Vec<BatchUpdateAnnotation>,
    #[serde(default)]
    pub delete: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchCreateAnnotation {
    pub slide_id: String,
    #[serde(flatten)]
    pub annotation: CreateAnnotationRequest,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchUpdateAnnotation {
    pub id: String,
    #[serde(flatten)]
    pub annotation: UpdateAnnotationRequest,
}

#[derive(Debug, Serialize)]
pub struct BatchAnnotationResponse {
    pub created: Vec<Annotation>,
    pub updated: Vec<Annotation>,
    pub deleted: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationFormat {
    Json,
    Geojson,
    Xml,
    Aperio,
}

#[derive(Debug, Deserialize)]
pub struct FormatQuery {
    #[serde(default = "default_format")]
    pub format: AnnotationFormat,
}

fn default_format() -> AnnotationFormat {
    AnnotationFormat::Json
}

#[derive(Clone)]
pub struct AnnotationStore {
    inner: Arc<RwLock<StoreInner>>,
    file_path: Option<PathBuf>,
    id_counter: Arc<AtomicU64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoreInner {
    annotations: HashMap<String, Annotation>,
    slide_index: HashMap<String, HashSet<String>>,
    spatial_index: HashMap<String, Vec<SpatialEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpatialEntry {
    id: String,
    bbox: BoundingBox,
}

impl Default for AnnotationStore {
    fn default() -> Self {
        Self::in_memory()
    }
}

impl AnnotationStore {
    pub fn in_memory() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreInner::default())),
            file_path: None,
            id_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn json_file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut inner = StoreInner::default();
        if path.exists() {
            if let Ok(bytes) = std::fs::read(&path) {
                if !bytes.is_empty() {
                    if let Ok(mut loaded) = serde_json::from_slice::<StoreInner>(&bytes) {
                        rebuild_indexes(&mut loaded);
                        inner = loaded;
                    }
                }
            }
        }
        Self {
            inner: Arc::new(RwLock::new(inner)),
            file_path: Some(path),
            id_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    pub async fn load(path: impl Into<PathBuf>) -> Result<Self, AnnotationError> {
        let path = path.into();
        let store = Self::json_file(path.clone());
        if path.exists() {
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|e| AnnotationError::Storage(e.to_string()))?;
            if !bytes.is_empty() {
                let mut inner: StoreInner = serde_json::from_slice(&bytes)
                    .map_err(|e| AnnotationError::Storage(e.to_string()))?;
                rebuild_indexes(&mut inner);
                *store.inner.write().await = inner;
            }
        }
        Ok(store)
    }

    pub async fn create(
        &self,
        slide_id: String,
        request: CreateAnnotationRequest,
    ) -> Result<Annotation, AnnotationError> {
        request.geometry.validate()?;
        validate_style(&request.style)?;
        validate_required("slide_id", &slide_id)?;
        validate_required("author_id", &request.author_id)?;

        let now = unix_time();
        let annotation = Annotation {
            id: request.id.unwrap_or_else(|| self.next_id()),
            slide_id,
            annotation_type: request.geometry.annotation_type(),
            bbox: request.geometry.bbox(),
            geometry: request.geometry,
            style: request.style,
            label: request.label,
            created_at: now,
            updated_at: now,
            author_id: request.author_id,
        };

        let snapshot = {
            let mut inner = self.inner.write().await;
            if inner.annotations.contains_key(&annotation.id) {
                return Err(AnnotationError::Invalid(format!(
                    "annotation id already exists: {}",
                    annotation.id
                )));
            }
            insert_indexes(&mut inner, &annotation);
            inner
                .annotations
                .insert(annotation.id.clone(), annotation.clone());
            serde_json::to_vec_pretty(&*inner).ok()
        };
        self.persist_snapshot(snapshot).await?;
        Ok(annotation)
    }

    pub async fn get(&self, id: &str) -> Result<Annotation, AnnotationError> {
        self.inner
            .read()
            .await
            .annotations
            .get(id)
            .cloned()
            .ok_or_else(|| AnnotationError::NotFound(id.to_string()))
    }

    pub async fn list(
        &self,
        slide_id: &str,
        query: &AnnotationQuery,
    ) -> Result<Vec<Annotation>, AnnotationError> {
        let viewport = match (query.x, query.y, query.width, query.height) {
            (Some(x), Some(y), Some(width), Some(height)) => {
                validate_positive("width", width)?;
                validate_positive("height", height)?;
                Some(BoundingBox {
                    x,
                    y,
                    width,
                    height,
                })
            }
            (None, None, None, None) => None,
            _ => {
                return Err(AnnotationError::Invalid(
                    "x, y, width and height must be provided together".to_string(),
                ));
            }
        };

        let inner = self.inner.read().await;
        let ids: Vec<String> = if let Some(viewport) = viewport {
            inner
                .spatial_index
                .get(slide_id)
                .into_iter()
                .flat_map(|entries| entries.iter())
                .filter(|entry| entry.bbox.intersects(&viewport))
                .map(|entry| entry.id.clone())
                .collect()
        } else {
            inner
                .slide_index
                .get(slide_id)
                .into_iter()
                .flat_map(|set| set.iter().cloned())
                .collect()
        };

        let mut annotations: Vec<Annotation> = ids
            .iter()
            .filter_map(|id| inner.annotations.get(id))
            .filter(|annotation| {
                query
                    .annotation_type
                    .as_ref()
                    .map(|t| &annotation.annotation_type == t)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        annotations.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));

        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(annotations.len()).min(10_000);
        Ok(annotations.into_iter().skip(offset).take(limit).collect())
    }

    pub async fn update(
        &self,
        id: &str,
        request: UpdateAnnotationRequest,
    ) -> Result<Annotation, AnnotationError> {
        if let Some(style) = &request.style {
            validate_style(style)?;
        }
        if let Some(geometry) = &request.geometry {
            geometry.validate()?;
        }

        let updated = {
            let mut inner = self.inner.write().await;
            let mut annotation = inner
                .annotations
                .get(id)
                .cloned()
                .ok_or_else(|| AnnotationError::NotFound(id.to_string()))?;

            remove_indexes(&mut inner, &annotation);
            if let Some(geometry) = request.geometry {
                annotation.annotation_type = geometry.annotation_type();
                annotation.bbox = geometry.bbox();
                annotation.geometry = geometry;
            }
            if let Some(style) = request.style {
                annotation.style = style;
            }
            if request.label.is_some() {
                annotation.label = request.label;
            }
            if let Some(author_id) = request.author_id {
                validate_required("author_id", &author_id)?;
                annotation.author_id = author_id;
            }
            annotation.updated_at = unix_time();

            insert_indexes(&mut inner, &annotation);
            inner
                .annotations
                .insert(annotation.id.clone(), annotation.clone());
            (annotation, serde_json::to_vec_pretty(&*inner).ok())
        };
        self.persist_snapshot(updated.1).await?;
        Ok(updated.0)
    }

    pub async fn delete(&self, id: &str) -> Result<(), AnnotationError> {
        let snapshot = {
            let mut inner = self.inner.write().await;
            let annotation = inner
                .annotations
                .remove(id)
                .ok_or_else(|| AnnotationError::NotFound(id.to_string()))?;
            remove_indexes(&mut inner, &annotation);
            serde_json::to_vec_pretty(&*inner).ok()
        };
        self.persist_snapshot(snapshot).await
    }

    pub async fn batch(
        &self,
        request: BatchAnnotationRequest,
    ) -> Result<BatchAnnotationResponse, AnnotationError> {
        let mut created = Vec::new();
        for item in request.create {
            created.push(self.create(item.slide_id, item.annotation).await?);
        }

        let mut updated = Vec::new();
        for item in request.update {
            updated.push(self.update(&item.id, item.annotation).await?);
        }

        let mut deleted = Vec::new();
        for id in request.delete {
            self.delete(&id).await?;
            deleted.push(id);
        }

        Ok(BatchAnnotationResponse {
            created,
            updated,
            deleted,
        })
    }

    pub async fn export_slide(
        &self,
        slide_id: &str,
        format: AnnotationFormat,
    ) -> Result<String, AnnotationError> {
        let annotations = self
            .list(
                slide_id,
                &AnnotationQuery {
                    annotation_type: None,
                    x: None,
                    y: None,
                    width: None,
                    height: None,
                    limit: None,
                    offset: None,
                },
            )
            .await?;

        match format {
            AnnotationFormat::Json => serde_json::to_string_pretty(&annotations)
                .map_err(|e| AnnotationError::Storage(e.to_string())),
            AnnotationFormat::Geojson => export_geojson(&annotations),
            AnnotationFormat::Xml | AnnotationFormat::Aperio => Ok(export_aperio_xml(&annotations)),
        }
    }

    fn next_id(&self) -> String {
        let counter = self.id_counter.fetch_add(1, Ordering::SeqCst);
        format!("ann-{}-{}", unix_time_millis(), counter)
    }

    async fn persist_snapshot(&self, snapshot: Option<Vec<u8>>) -> Result<(), AnnotationError> {
        let Some(path) = &self.file_path else {
            return Ok(());
        };
        let Some(bytes) = snapshot else {
            return Err(AnnotationError::Storage(
                "failed to serialize annotation store".to_string(),
            ));
        };
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AnnotationError::Storage(e.to_string()))?;
        }
        tokio::fs::write(path, bytes)
            .await
            .map_err(|e| AnnotationError::Storage(e.to_string()))
    }
}

pub fn parse_import_payload(
    slide_id: &str,
    format: AnnotationFormat,
    payload: &[u8],
    author_id: String,
) -> Result<Vec<CreateAnnotationRequest>, AnnotationError> {
    match format {
        AnnotationFormat::Json => {
            let mut values: Vec<Annotation> = serde_json::from_slice(payload)
                .map_err(|e| AnnotationError::Invalid(e.to_string()))?;
            Ok(values
                .drain(..)
                .filter(|annotation| annotation.slide_id == slide_id)
                .map(|annotation| CreateAnnotationRequest {
                    id: Some(annotation.id),
                    geometry: annotation.geometry,
                    style: annotation.style,
                    label: annotation.label,
                    author_id: annotation.author_id,
                })
                .collect())
        }
        AnnotationFormat::Geojson => import_geojson(payload, author_id),
        AnnotationFormat::Xml | AnnotationFormat::Aperio => {
            Err(AnnotationError::UnsupportedFormat(
                "Aperio XML import is not implemented; export is supported".to_string(),
            ))
        }
    }
}

fn points_bbox(points: &[Point]) -> BoundingBox {
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    BoundingBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

fn validate_point(point: &Point) -> Result<(), AnnotationError> {
    validate_number("x", point.x)?;
    validate_number("y", point.y)
}

fn validate_points(points: &[Point], min: usize, message: &str) -> Result<(), AnnotationError> {
    if points.len() < min {
        return Err(AnnotationError::Invalid(message.to_string()));
    }
    for point in points {
        validate_point(point)?;
    }
    Ok(())
}

fn validate_number(field: &str, value: f64) -> Result<(), AnnotationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(AnnotationError::Invalid(format!(
            "{} must be a finite number",
            field
        )))
    }
}

fn validate_positive(field: &str, value: f64) -> Result<(), AnnotationError> {
    validate_number(field, value)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(AnnotationError::Invalid(format!(
            "{} must be greater than 0",
            field
        )))
    }
}

fn validate_required(field: &str, value: &str) -> Result<(), AnnotationError> {
    if value.trim().is_empty() {
        Err(AnnotationError::Invalid(format!("{} is required", field)))
    } else {
        Ok(())
    }
}

fn validate_style(style: &AnnotationStyle) -> Result<(), AnnotationError> {
    if !(0.0..=1.0).contains(&style.opacity) {
        return Err(AnnotationError::Invalid(
            "opacity must be between 0 and 1".to_string(),
        ));
    }
    if !style.color.starts_with('#') || !(style.color.len() == 7 || style.color.len() == 9) {
        return Err(AnnotationError::Invalid(
            "color must be a #RRGGBB or #RRGGBBAA hex value".to_string(),
        ));
    }
    if let Some(point_radius) = style.point_radius {
        validate_positive("point_radius", point_radius)?;
    }
    Ok(())
}

fn insert_indexes(inner: &mut StoreInner, annotation: &Annotation) {
    inner
        .slide_index
        .entry(annotation.slide_id.clone())
        .or_default()
        .insert(annotation.id.clone());
    inner
        .spatial_index
        .entry(annotation.slide_id.clone())
        .or_default()
        .push(SpatialEntry {
            id: annotation.id.clone(),
            bbox: annotation.bbox,
        });
}

fn remove_indexes(inner: &mut StoreInner, annotation: &Annotation) {
    if let Some(ids) = inner.slide_index.get_mut(&annotation.slide_id) {
        ids.remove(&annotation.id);
    }
    if let Some(entries) = inner.spatial_index.get_mut(&annotation.slide_id) {
        entries.retain(|entry| entry.id != annotation.id);
    }
}

fn rebuild_indexes(inner: &mut StoreInner) {
    inner.slide_index.clear();
    inner.spatial_index.clear();
    let annotations: Vec<Annotation> = inner.annotations.values().cloned().collect();
    for annotation in annotations {
        insert_indexes(inner, &annotation);
    }
}

fn export_geojson(annotations: &[Annotation]) -> Result<String, AnnotationError> {
    let features: Vec<_> = annotations
        .iter()
        .map(|annotation| {
            json!({
                "type": "Feature",
                "id": annotation.id,
                "properties": {
                    "slide_id": annotation.slide_id,
                    "annotation_type": annotation.annotation_type,
                    "color": annotation.style.color,
                    "opacity": annotation.style.opacity,
                    "label": annotation.label,
                    "created_at": annotation.created_at,
                    "updated_at": annotation.updated_at,
                    "author_id": annotation.author_id
                },
                "geometry": geojson_geometry(&annotation.geometry)
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({
        "type": "FeatureCollection",
        "features": features
    }))
    .map_err(|e| AnnotationError::Storage(e.to_string()))
}

fn geojson_geometry(geometry: &Geometry) -> serde_json::Value {
    match geometry {
        Geometry::Point { point } => json!({"type": "Point", "coordinates": [point.x, point.y]}),
        Geometry::Rectangle {
            x,
            y,
            width,
            height,
        } => {
            let points = vec![
                vec![*x, *y],
                vec![*x + *width, *y],
                vec![*x + *width, *y + *height],
                vec![*x, *y + *height],
                vec![*x, *y],
            ];
            json!({"type": "Polygon", "coordinates": [points]})
        }
        Geometry::Polygon { points } => {
            let mut coords: Vec<Vec<f64>> = points.iter().map(|p| vec![p.x, p.y]).collect();
            if coords.first() != coords.last() {
                if let Some(first) = coords.first().cloned() {
                    coords.push(first);
                }
            }
            json!({"type": "Polygon", "coordinates": [coords]})
        }
        Geometry::Circle { center, radius } => json!({
            "type": "Point",
            "coordinates": [center.x, center.y],
            "radius": radius
        }),
        Geometry::Ellipse {
            center,
            radius_x,
            radius_y,
        } => json!({
            "type": "Point",
            "coordinates": [center.x, center.y],
            "radius_x": radius_x,
            "radius_y": radius_y
        }),
        Geometry::Line { start, end } => {
            json!({"type": "LineString", "coordinates": [[start.x, start.y], [end.x, end.y]]})
        }
        Geometry::Polyline { points } => {
            let coords: Vec<Vec<f64>> = points.iter().map(|p| vec![p.x, p.y]).collect();
            json!({"type": "LineString", "coordinates": coords})
        }
    }
}

fn import_geojson(
    payload: &[u8],
    author_id: String,
) -> Result<Vec<CreateAnnotationRequest>, AnnotationError> {
    let value: serde_json::Value =
        serde_json::from_slice(payload).map_err(|e| AnnotationError::Invalid(e.to_string()))?;
    let features = value
        .get("features")
        .and_then(|f| f.as_array())
        .ok_or_else(|| {
            AnnotationError::Invalid("GeoJSON FeatureCollection expected".to_string())
        })?;

    features
        .iter()
        .map(|feature| {
            let properties = feature
                .get("properties")
                .unwrap_or(&serde_json::Value::Null);
            let geometry = feature
                .get("geometry")
                .ok_or_else(|| AnnotationError::Invalid("feature.geometry is required".to_string()))
                .and_then(geometry_from_geojson)?;
            let style = AnnotationStyle {
                color: properties
                    .get("color")
                    .and_then(|v| v.as_str())
                    .unwrap_or("#ff3366")
                    .to_string(),
                opacity: properties
                    .get("opacity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.55) as f32,
                point_radius: properties.get("point_radius").and_then(|v| v.as_f64()),
            };
            Ok(CreateAnnotationRequest {
                id: feature
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                geometry,
                style,
                label: properties
                    .get("label")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                author_id: properties
                    .get("author_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&author_id)
                    .to_string(),
            })
        })
        .collect()
}

fn geometry_from_geojson(value: &serde_json::Value) -> Result<Geometry, AnnotationError> {
    let geometry_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AnnotationError::Invalid("geometry.type is required".to_string()))?;
    match geometry_type {
        "Point" => {
            let coords = coordinates_pair(value.get("coordinates"))?;
            if let Some(radius) = value.get("radius").and_then(|v| v.as_f64()) {
                Ok(Geometry::Circle {
                    center: coords,
                    radius,
                })
            } else if let (Some(radius_x), Some(radius_y)) = (
                value.get("radius_x").and_then(|v| v.as_f64()),
                value.get("radius_y").and_then(|v| v.as_f64()),
            ) {
                Ok(Geometry::Ellipse {
                    center: coords,
                    radius_x,
                    radius_y,
                })
            } else {
                Ok(Geometry::Point { point: coords })
            }
        }
        "LineString" => {
            let points = coordinates_list(value.get("coordinates"))?;
            if points.len() == 2 {
                Ok(Geometry::Line {
                    start: points[0],
                    end: points[1],
                })
            } else {
                Ok(Geometry::Polyline { points })
            }
        }
        "Polygon" => {
            let ring = value
                .get("coordinates")
                .and_then(|v| v.as_array())
                .and_then(|rings| rings.first())
                .ok_or_else(|| AnnotationError::Invalid("polygon ring is required".to_string()))?;
            let mut points = coordinates_list(Some(ring))?;
            if points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            Ok(Geometry::Polygon { points })
        }
        other => Err(AnnotationError::UnsupportedFormat(format!(
            "unsupported GeoJSON geometry: {}",
            other
        ))),
    }
}

fn coordinates_pair(value: Option<&serde_json::Value>) -> Result<Point, AnnotationError> {
    let coords = value
        .and_then(|v| v.as_array())
        .ok_or_else(|| AnnotationError::Invalid("coordinate pair expected".to_string()))?;
    let x = coords
        .first()
        .and_then(|v| v.as_f64())
        .ok_or_else(|| AnnotationError::Invalid("x coordinate expected".to_string()))?;
    let y = coords
        .get(1)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| AnnotationError::Invalid("y coordinate expected".to_string()))?;
    Ok(Point { x, y })
}

fn coordinates_list(value: Option<&serde_json::Value>) -> Result<Vec<Point>, AnnotationError> {
    let coords = value
        .and_then(|v| v.as_array())
        .ok_or_else(|| AnnotationError::Invalid("coordinates array expected".to_string()))?;
    coords
        .iter()
        .map(|coord| coordinates_pair(Some(coord)))
        .collect()
}

fn export_aperio_xml(annotations: &[Annotation]) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><Annotations>"#);
    for (index, annotation) in annotations.iter().enumerate() {
        xml.push_str(&format!(
            r#"<Annotation Id="{}" Name="{}" Type="{}" PartOfGroup="None" Color="{}"><Regions><Region Id="{}" Type="0"><Vertices>"#,
            xml_escape(&annotation.id),
            xml_escape(annotation.label.as_deref().unwrap_or("")),
            xml_escape(&format!("{:?}", annotation.annotation_type)),
            xml_escape(&annotation.style.color),
            index + 1
        ));
        for point in aperio_vertices(&annotation.geometry) {
            xml.push_str(&format!(
                r#"<Vertex X="{:.3}" Y="{:.3}" Z="0"/>"#,
                point.x, point.y
            ));
        }
        xml.push_str("</Vertices></Region></Regions></Annotation>");
    }
    xml.push_str("</Annotations>");
    xml
}

fn aperio_vertices(geometry: &Geometry) -> Vec<Point> {
    match geometry {
        Geometry::Point { point } => vec![*point],
        Geometry::Rectangle {
            x,
            y,
            width,
            height,
        } => vec![
            Point { x: *x, y: *y },
            Point {
                x: *x + *width,
                y: *y,
            },
            Point {
                x: *x + *width,
                y: *y + *height,
            },
            Point {
                x: *x,
                y: *y + *height,
            },
        ],
        Geometry::Polygon { points } | Geometry::Polyline { points } => points.clone(),
        Geometry::Circle { center, radius } => circle_vertices(center, *radius, *radius),
        Geometry::Ellipse {
            center,
            radius_x,
            radius_y,
        } => circle_vertices(center, *radius_x, *radius_y),
        Geometry::Line { start, end } => vec![*start, *end],
    }
}

fn circle_vertices(center: &Point, radius_x: f64, radius_y: f64) -> Vec<Point> {
    (0..32)
        .map(|i| {
            let angle = (i as f64 / 32.0) * std::f64::consts::TAU;
            Point {
                x: center.x + radius_x * angle.cos(),
                y: center.y + radius_y * angle.sin(),
            }
        })
        .collect()
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn unix_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn store_filters_annotations_by_viewport() {
        let store = AnnotationStore::in_memory();
        store
            .create(
                "slide".to_string(),
                CreateAnnotationRequest {
                    id: Some("a".to_string()),
                    geometry: Geometry::Rectangle {
                        x: 10.0,
                        y: 10.0,
                        width: 20.0,
                        height: 20.0,
                    },
                    style: AnnotationStyle::default(),
                    label: None,
                    author_id: "user".to_string(),
                },
            )
            .await
            .unwrap();

        let found = store
            .list(
                "slide",
                &AnnotationQuery {
                    annotation_type: Some(AnnotationType::Rectangle),
                    x: Some(0.0),
                    y: Some(0.0),
                    width: Some(15.0),
                    height: Some(15.0),
                    limit: None,
                    offset: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn invalid_polygon_is_rejected() {
        let geometry = Geometry::Polygon {
            points: vec![Point { x: 1.0, y: 2.0 }],
        };
        assert!(geometry.validate().is_err());
    }
}
