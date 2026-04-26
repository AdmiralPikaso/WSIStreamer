use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use wsi_streamer::slide::SlideRegistry;
use wsi_streamer::tile::TileService;
use wsi_streamer::{create_router, RouterConfig};

use super::test_utils::MockSlideSource;

fn test_router() -> axum::Router {
    let source = MockSlideSource::new();
    let registry = SlideRegistry::new(source);
    let tile_service = TileService::new(registry);
    create_router(tile_service, RouterConfig::without_auth())
}

#[tokio::test]
async fn annotation_crud_and_viewport_filter_work() {
    let router = test_router();
    let payload = r##"{
        "geometry": {
            "kind": "rectangle",
            "x": 10.0,
            "y": 20.0,
            "width": 100.0,
            "height": 50.0
        },
        "style": { "color": "#ff3366", "opacity": 0.5 },
        "label": "tumor",
        "author_id": "pathologist"
    }"##;

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slides/test.tif/annotations")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["annotation_type"], "rectangle");

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/slides/test.tif/annotations?x=0&y=0&width=20&height=30")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let listed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let update = r##"{
        "geometry": { "kind": "point", "point": { "x": 5.0, "y": 6.0 } },
        "style": { "color": "#00aa88", "opacity": 0.75 },
        "label": "updated",
        "author_id": "pathologist"
    }"##;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/annotations/{}", id))
                .header("content-type", "application/json")
                .body(Body::from(update))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let updated: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["annotation_type"], "point");

    let response = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/annotations/{}", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn batch_and_geojson_export_work() {
    let router = test_router();
    let payload = r##"{
        "create": [{
            "slide_id": "slide-a",
            "geometry": { "kind": "point", "point": { "x": 1.0, "y": 2.0 } },
            "style": { "color": "#ff3366", "opacity": 0.5 },
            "label": "a",
            "author_id": "user"
        }]
    }"##;

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/annotations/batch")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/slides/slide-a/annotations/export?format=geojson")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/geo+json"
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let geojson: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(geojson["type"], "FeatureCollection");
    assert_eq!(geojson["features"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_annotation_returns_bad_request() {
    let router = test_router();
    let payload = r##"{
        "geometry": {
            "kind": "polygon",
            "points": [{ "x": 1.0, "y": 2.0 }]
        },
        "style": { "color": "#ff3366", "opacity": 0.5 },
        "author_id": "pathologist"
    }"##;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slides/test.tif/annotations")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
