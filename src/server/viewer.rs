//! Viewer module - generates HTML pages for viewing slides with OpenSeadragon.

use crate::server::handlers::SlideMetadataResponse;

/// Escape HTML special characters to prevent XSS attacks.
fn html_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#x27;"),
            _ => result.push(c),
        }
    }
    result
}

/// Generate an HTML page with OpenSeadragon viewer for a slide.
///
/// # Arguments
///
/// * `slide_id` - The slide identifier (will be URL-encoded in tile URLs)
/// * `metadata` - Slide metadata containing dimensions and level info
/// * `base_url` - Base URL for tile requests (e.g., "http://localhost:3000")
/// * `auth_query` - Optional query string for authentication (e.g., "&exp=...&sig=...")
/// * `author_id` - Default annotation author id for the viewer.
pub fn generate_viewer_html(
    slide_id: &str,
    metadata: &SlideMetadataResponse,
    base_url: &str,
    auth_query: &str,
    author_id: &str,
) -> String {
    let base_url = base_url.trim_end_matches('/');
    let encoded_slide_id = urlencoding::encode(slide_id);

    // Get tile size from level 0 (or default to 256)
    let tile_size = metadata.levels.first().map(|l| l.tile_width).unwrap_or(256);

    // Calculate max level for OpenSeadragon (OSD uses inverted levels)
    let actual_level_count = metadata.levels.len();
    let max_level = actual_level_count.saturating_sub(1);

    // Build level dimensions JSON for the tile source
    // Include original level index to handle filtered levels correctly
    let level_dimensions: Vec<String> = metadata
        .levels
        .iter()
        .map(|l| {
            format!(
                "{{ level: {}, width: {}, height: {} }}",
                l.level, l.width, l.height
            )
        })
        .collect();

    // Escape user-controlled values to prevent XSS
    let escaped_slide_id = html_escape(slide_id);
    let escaped_format = html_escape(&metadata.format);
    let annotation_styles = annotation_styles();
    let annotation_panel = annotation_panel();
    let annotation_script = annotation_script(base_url, &encoded_slide_id, auth_query, author_id);

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>WSI Viewer - {escaped_slide_id}</title>
    <script src="https://cdn.jsdelivr.net/npm/openseadragon@4.1/build/openseadragon/openseadragon.min.js"></script>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        body {{
            background: #0f0f0f;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            overflow: hidden;
        }}
        #viewer {{
            width: 100vw;
            height: 100vh;
            position: relative;
        }}
        .info-panel {{
            position: absolute;
            top: 16px;
            left: 16px;
            background: rgba(0, 0, 0, 0.85);
            color: #fff;
            padding: 16px 20px;
            border-radius: 8px;
            font-size: 13px;
            line-height: 1.5;
            backdrop-filter: blur(10px);
            border: 1px solid rgba(255, 255, 255, 0.1);
            max-width: 320px;
            z-index: 1000;
        }}
        .info-panel h2 {{
            font-size: 14px;
            font-weight: 600;
            margin-bottom: 8px;
            color: #fff;
            word-break: break-all;
        }}
        .info-panel .meta {{
            color: rgba(255, 255, 255, 0.7);
            font-size: 12px;
        }}
        .info-panel .meta span {{
            color: rgba(255, 255, 255, 0.9);
        }}
        .info-panel .format-badge {{
            display: inline-block;
            background: rgba(99, 102, 241, 0.2);
            color: #818cf8;
            padding: 2px 8px;
            border-radius: 4px;
            font-size: 11px;
            font-weight: 500;
            margin-top: 8px;
        }}
        .controls-hint {{
            position: absolute;
            bottom: 16px;
            left: 16px;
            background: rgba(0, 0, 0, 0.7);
            color: rgba(255, 255, 255, 0.6);
            padding: 8px 12px;
            border-radius: 6px;
            font-size: 11px;
            backdrop-filter: blur(10px);
        }}
        .controls-hint kbd {{
            background: rgba(255, 255, 255, 0.15);
            padding: 2px 6px;
            border-radius: 3px;
            margin: 0 2px;
        }}
        .loading {{
            position: absolute;
            top: 50%;
            left: 50%;
            transform: translate(-50%, -50%);
            color: rgba(255, 255, 255, 0.5);
            font-size: 14px;
        }}
        .error-banner {{
            position: absolute;
            top: 0;
            left: 0;
            right: 0;
            background: rgba(220, 38, 38, 0.95);
            color: white;
            padding: 12px 20px;
            font-size: 14px;
            z-index: 1000;
            display: none;
            backdrop-filter: blur(10px);
        }}
        .error-banner.visible {{
            display: block;
        }}
        .error-banner strong {{
            font-weight: 600;
        }}
        .error-banner .error-details {{
            font-size: 12px;
            opacity: 0.9;
            margin-top: 4px;
        }}
        {annotation_styles}
    </style>
</head>
<body>
    <div id="error-banner" class="error-banner">
        <strong>Failed to load tiles</strong>
        <div class="error-details" id="error-details"></div>
    </div>

    <div id="viewer">
        <div class="loading">Loading slide...</div>
    </div>

    <div class="info-panel">
        <h2>{escaped_slide_id}</h2>
        <div class="meta">
            <span>{width}</span> x <span>{height}</span> px<br>
            <span>{level_count}</span> pyramid levels<br>
            Tile size: <span>{tile_size}</span> px
        </div>
        <div class="format-badge">{escaped_format}</div>
    </div>

    <div class="controls-hint">
        <kbd>+</kbd>/<kbd>-</kbd> Zoom &nbsp; <kbd>Home</kbd> Reset &nbsp; <kbd>F</kbd> Fullscreen
    </div>

    {annotation_panel}

    <script>
        // Level dimensions from server metadata
        const levelDimensions = [{level_dimensions}];
        const levelCount = {level_count};
        const maxLevel = {max_level};

        // Check if OpenSeadragon loaded
        if (typeof OpenSeadragon === 'undefined') {{
            document.querySelector('.loading').textContent = 'Error: Viewer library failed to load.';
            throw new Error('OpenSeadragon library not loaded');
        }}

        // Create custom tile source
        const tileSource = {{
            height: {height},
            width: {width},
            tileSize: {tile_size},
            minLevel: 0,
            maxLevel: maxLevel,

            getLevelScale: function(level) {{
                // OpenSeadragon level 0 is lowest resolution, but we want level 0 to be highest
                // So we need to invert: OSD level N maps to our level (maxLevel - N)
                const ourLevel = maxLevel - level;
                if (ourLevel < 0 || ourLevel >= levelCount) return 0;
                return levelDimensions[ourLevel].width / {width};
            }},

            getNumTiles: function(level) {{
                const ourLevel = maxLevel - level;
                if (ourLevel < 0 || ourLevel >= levelCount) return {{ x: 0, y: 0 }};
                const dims = levelDimensions[ourLevel];
                return {{
                    x: Math.ceil(dims.width / {tile_size}),
                    y: Math.ceil(dims.height / {tile_size})
                }};
            }},

            getTileUrl: function(level, x, y) {{
                // Map OSD level to our pyramid level (inverted)
                const ourLevel = maxLevel - level;
                // Use original level index from metadata for tile request
                const originalLevel = levelDimensions[ourLevel].level;
                return "{base_url}/tiles/{encoded_slide_id}/" + originalLevel + "/" + x + "/" + y + ".jpg{auth_query}";
            }}
        }};

        // Initialize OpenSeadragon
        const viewer = OpenSeadragon({{
            id: "viewer",
            prefixUrl: "https://cdn.jsdelivr.net/npm/openseadragon@4.1/build/openseadragon/images/",
            tileSources: tileSource,
            showNavigator: true,
            navigatorPosition: "BOTTOM_RIGHT",
            navigatorSizeRatio: 0.15,
            showRotationControl: true,
            showFullPageControl: true,
            showZoomControl: true,
            showHomeControl: true,
            gestureSettingsMouse: {{
                clickToZoom: true,
                dblClickToZoom: true,
                scrollToZoom: true
            }},
            gestureSettingsTouch: {{
                pinchToZoom: true
            }},
            animationTime: 0.3,
            blendTime: 0.1,
            maxZoomPixelRatio: 2,
            visibilityRatio: 0.5,
            constrainDuringPan: true,
            immediateRender: false,
            crossOriginPolicy: "Anonymous"
        }});

        console.log('[DEBUG] OpenSeadragon viewer created');

        // Track errors
        let errorCount = 0;
        let firstErrorShown = false;

        // Remove loading message when first tile loads
        viewer.addHandler('tile-loaded', function() {{
            const loading = document.querySelector('.loading');
            if (loading) loading.remove();
        }});

        // Handle tile load errors
        viewer.addHandler('tile-load-failed', function(event) {{
            errorCount++;

            // Show error banner on first failure
            if (!firstErrorShown) {{
                firstErrorShown = true;
                const banner = document.getElementById('error-banner');
                const details = document.getElementById('error-details');

                // Try to determine the error type
                let errorMessage = 'Unable to load slide tiles. ';
                if (event.message) {{
                    if (event.message.includes('401')) {{
                        errorMessage += 'Authentication failed - the viewer token may have expired. Try refreshing the page.';
                    }} else if (event.message.includes('404')) {{
                        errorMessage += 'Tile not found - the slide may have been moved or deleted.';
                    }} else if (event.message.includes('415')) {{
                        errorMessage += 'Unsupported slide format.';
                    }} else {{
                        errorMessage += event.message;
                    }}
                }} else {{
                    errorMessage += 'Check your network connection and try refreshing the page.';
                }}

                details.textContent = errorMessage;
                banner.classList.add('visible');

                // Also update loading message
                const loading = document.querySelector('.loading');
                if (loading) {{
                    loading.textContent = 'Error loading slide';
                    loading.style.color = 'rgba(220, 38, 38, 0.8)';
                }}
            }}
        }});

        {annotation_script}

        // Keyboard shortcuts
        document.addEventListener('keydown', function(e) {{
            const target = e.target;
            if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) {{
                return;
            }}
            if (e.key === 'f' || e.key === 'F') {{
                if (viewer.isFullPage()) {{
                    viewer.setFullPage(false);
                }} else {{
                    viewer.setFullPage(true);
                }}
            }}
        }});
    </script>
</body>
</html>"##,
        escaped_slide_id = escaped_slide_id,
        escaped_format = escaped_format,
        width = metadata.width,
        height = metadata.height,
        level_count = actual_level_count,
        tile_size = tile_size,
        level_dimensions = level_dimensions.join(", "),
        max_level = max_level,
        base_url = base_url,
        encoded_slide_id = encoded_slide_id,
        auth_query = auth_query,
        annotation_styles = annotation_styles,
        annotation_panel = annotation_panel,
        annotation_script = annotation_script,
    )
}

fn annotation_styles() -> &'static str {
    r#"
        #annotation-canvas {
            position: absolute;
            inset: 0;
            z-index: 20;
            pointer-events: auto;
        }
        .annotation-panel {
            position: absolute;
            top: 16px;
            right: 16px;
            z-index: 1001;
            display: grid;
            grid-template-columns: repeat(4, 34px);
            gap: 8px;
            align-items: center;
            background: rgba(12, 12, 12, 0.88);
            color: #fff;
            padding: 12px;
            border: 1px solid rgba(255, 255, 255, 0.14);
            border-radius: 8px;
            backdrop-filter: blur(10px);
            width: 184px;
        }
        .annotation-panel button,
        .annotation-panel input,
        .annotation-panel select {
            height: 34px;
            border: 1px solid rgba(255, 255, 255, 0.18);
            background: rgba(255, 255, 255, 0.08);
            color: #fff;
            border-radius: 6px;
            font-size: 12px;
        }
        .annotation-panel button {
            cursor: pointer;
            font-weight: 600;
        }
        .annotation-panel button.active {
            background: rgba(20, 184, 166, 0.35);
            border-color: rgba(45, 212, 191, 0.8);
        }
        .annotation-panel select,
        .annotation-panel .annotation-label,
        .annotation-panel .annotation-field,
        .annotation-panel .annotation-status {
            grid-column: 1 / -1;
            width: 100%;
        }
        .annotation-panel .annotation-field {
            display: grid;
            gap: 4px;
        }
        .annotation-panel .annotation-field label {
            color: rgba(255, 255, 255, 0.66);
            font-size: 11px;
            line-height: 12px;
        }
        .annotation-panel .annotation-label,
        .annotation-panel .annotation-field input {
            padding: 0 8px;
        }
        .annotation-panel .annotation-field input[readonly] {
            color: rgba(255, 255, 255, 0.72);
            background: rgba(255, 255, 255, 0.04);
            cursor: default;
        }
        .annotation-panel .annotation-actions {
            grid-column: 1 / -1;
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 8px;
        }
        .annotation-panel .annotation-status {
            min-height: 16px;
            color: rgba(255, 255, 255, 0.68);
            font-size: 11px;
            line-height: 16px;
            padding: 0;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
        }
        .annotation-panel .annotation-color {
            width: 34px;
            padding: 2px;
        }
        .annotation-panel .annotation-opacity {
            grid-column: span 3;
            width: 100%;
        }
        .annotation-tooltip {
            position: absolute;
            z-index: 1002;
            display: none;
            max-width: 240px;
            padding: 6px 8px;
            border-radius: 6px;
            background: rgba(0, 0, 0, 0.86);
            color: #fff;
            font-size: 12px;
            pointer-events: none;
            white-space: nowrap;
        }
        .annotation-minimap {
            position: absolute;
            right: 16px;
            bottom: 188px;
            z-index: 1001;
            width: 184px;
            height: 132px;
            border: 1px solid rgba(255, 255, 255, 0.18);
            border-radius: 8px;
            background: rgba(12, 12, 12, 0.78);
        }
        .annotation-minimap canvas {
            width: 100%;
            height: 100%;
            display: block;
        }
    "#
}

fn annotation_panel() -> &'static str {
    r##"
    <div class="annotation-panel" aria-label="Annotation tools">
        <button type="button" class="tool active" data-tool="point" title="Point">Pt</button>
        <button type="button" class="tool" data-tool="rectangle" title="Rectangle">Rect</button>
        <button type="button" class="tool" data-tool="polygon" title="Polygon">Poly</button>
        <button type="button" class="tool" data-tool="ellipse" title="Ellipse">Oval</button>
        <button type="button" class="tool" data-tool="line" title="Line">Line</button>
        <button type="button" class="tool" data-tool="polyline" title="Polyline">Path</button>
        <button type="button" id="annotation-edit" title="Move selected annotation">Move</button>
        <button type="button" id="annotation-refresh" title="Reload annotations">Sync</button>
        <input type="color" id="annotation-color" class="annotation-color" value="#ff3366" title="Color">
        <input type="range" id="annotation-opacity" class="annotation-opacity" min="0.1" max="1" step="0.05" value="0.55" title="Opacity">
        <div class="annotation-field">
            <label for="annotation-label">Label</label>
            <input type="text" id="annotation-label" maxlength="120" placeholder="Text for annotation" title="Text label">
        </div>
        <div class="annotation-field">
            <label for="annotation-author">Author</label>
            <input type="text" id="annotation-author" maxlength="120" placeholder="Author ID" title="Author ID" readonly>
        </div>
        <div class="annotation-actions">
            <button type="button" id="annotation-finish" title="Finish polygon or path">Done</button>
            <button type="button" id="annotation-cancel" title="Cancel current drawing">Cancel</button>
        </div>
        <div id="annotation-status" class="annotation-status"></div>
    </div>
    <canvas id="annotation-canvas"></canvas>
    <div id="annotation-tooltip" class="annotation-tooltip"></div>
    <div class="annotation-minimap"><canvas id="annotation-minimap-canvas"></canvas></div>
    "##
}

fn annotation_script(
    base_url: &str,
    encoded_slide_id: &str,
    auth_query: &str,
    author_id: &str,
) -> String {
    let script = r#"
        const annotationApiBase = "__BASE_URL__";
        const annotationSlideId = "__SLIDE_ID__";
        const annotationAuthQuery = "__AUTH_QUERY__";
        const annotationDefaultAuthor = "__AUTHOR_ID__";
        const annotationCanvas = document.getElementById('annotation-canvas');
        const annotationCtx = annotationCanvas.getContext('2d');
        const annotationMinimap = document.getElementById('annotation-minimap-canvas');
        const annotationMinimapCtx = annotationMinimap.getContext('2d');
        const annotationTooltip = document.getElementById('annotation-tooltip');
        const annotationColor = document.getElementById('annotation-color');
        const annotationOpacity = document.getElementById('annotation-opacity');
        const annotationLabel = document.getElementById('annotation-label');
        const annotationAuthor = document.getElementById('annotation-author');
        const annotationStatus = document.getElementById('annotation-status');
        const annotationFinish = document.getElementById('annotation-finish');
        const annotationCancel = document.getElementById('annotation-cancel');
        viewer.container.appendChild(annotationCanvas);
        let annotations = [];
        let activeTool = 'point';
        let editMode = false;
        let draft = null;
        let selectedAnnotation = null;
        let dragStart = null;
        let editOperation = null;
        let resizeHandle = null;
        let resizeStart = null;
        annotationAuthor.value = annotationDefaultAuthor || 'viewer';

        function resizeAnnotationCanvas() {
            const rect = viewer.container.getBoundingClientRect();
            annotationCanvas.width = Math.max(1, Math.floor(rect.width * window.devicePixelRatio));
            annotationCanvas.height = Math.max(1, Math.floor(rect.height * window.devicePixelRatio));
            annotationCanvas.style.width = rect.width + 'px';
            annotationCanvas.style.height = rect.height + 'px';
            annotationCtx.setTransform(window.devicePixelRatio, 0, 0, window.devicePixelRatio, 0, 0);
            annotationMinimap.width = 184 * window.devicePixelRatio;
            annotationMinimap.height = 132 * window.devicePixelRatio;
            annotationMinimapCtx.setTransform(window.devicePixelRatio, 0, 0, window.devicePixelRatio, 0, 0);
            renderAnnotations();
        }

        function imageToCanvas(point) {
            return viewer.viewport.pixelFromPoint(viewer.viewport.imageToViewportCoordinates(point.x, point.y), true);
        }

        function canvasToImage(event) {
            const rect = annotationCanvas.getBoundingClientRect();
            const pixel = new OpenSeadragon.Point(event.clientX - rect.left, event.clientY - rect.top);
            const viewportPoint = viewer.viewport.pointFromPixel(pixel, true);
            const imagePoint = viewer.viewport.viewportToImageCoordinates(viewportPoint);
            return { x: imagePoint.x, y: imagePoint.y };
        }

        function currentStyle() {
            return { color: annotationColor.value, opacity: Number(annotationOpacity.value) };
        }

        function setStatus(message) {
            annotationStatus.textContent = message || '';
        }

        function annotationCountLabel(count) {
            return `${count} annotation${count === 1 ? '' : 's'}`;
        }

        function annotationEndpoint(path, query) {
            let url = `${annotationApiBase}${path}`;
            const params = new URLSearchParams(query || {});
            if (annotationAuthQuery) {
                const authParams = new URLSearchParams(annotationAuthQuery.slice(1));
                for (const [key, value] of authParams.entries()) params.set(key, value);
            }
            const queryString = params.toString();
            return queryString ? `${url}?${queryString}` : url;
        }

        async function loadAnnotations(showStatus = false) {
            const bounds = viewer.viewport.viewportToImageRectangle(viewer.viewport.getBounds(true));
            const query = {
                x: Math.max(0, bounds.x).toString(),
                y: Math.max(0, bounds.y).toString(),
                width: Math.max(1, bounds.width).toString(),
                height: Math.max(1, bounds.height).toString(),
                limit: '10000'
            };
            if (showStatus) setStatus('Syncing...');
            try {
                const response = await fetch(annotationEndpoint(`/slides/${annotationSlideId}/annotations`, query));
                if (!response.ok) throw new Error(`HTTP ${response.status}`);
                annotations = await response.json();
                selectedAnnotation = selectedAnnotation
                    ? annotations.find(annotation => annotation.id === selectedAnnotation.id) || null
                    : null;
                if (selectedAnnotation) populateControlsFromAnnotation(selectedAnnotation);
                if (showStatus) setStatus(`Synced ${annotationCountLabel(annotations.length)}`);
                renderAnnotations();
            } catch (error) {
                if (showStatus) setStatus(`Sync failed: ${error.message}`);
            }
        }

        async function saveAnnotation(geometry) {
            const label = annotationLabel.value.trim();
            const author = annotationDefaultAuthor || 'viewer';
            const response = await fetch(annotationEndpoint(`/slides/${annotationSlideId}/annotations`), {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    geometry,
                    style: currentStyle(),
                    label: label || null,
                    author_id: author
                })
            });
            if (response.ok) {
                const annotation = await response.json();
                annotations.push(annotation);
                selectedAnnotation = annotation;
                annotationLabel.value = '';
                annotationAuthor.value = annotation.author_id || author;
                setStatus(`Saved ${geometry.kind}`);
                renderAnnotations();
            } else {
                const error = await response.json().catch(() => ({ message: `HTTP ${response.status}` }));
                setStatus(error.message || 'Save failed');
            }
        }

        async function updateAnnotation(annotation) {
            const response = await fetch(annotationEndpoint(`/slides/${annotationSlideId}/annotations/${annotation.id}`), {
                method: 'PUT',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    geometry: annotation.geometry,
                    style: annotation.style,
                    label: annotation.label,
                    author_id: annotation.author_id
                })
            });
            if (response.ok) {
                setStatus(`Updated ${annotation.id}`);
            } else {
                setStatus(`Update failed: HTTP ${response.status}`);
            }
        }

        function populateControlsFromAnnotation(annotation) {
            annotationLabel.value = annotation.label || '';
            annotationColor.value = annotation.style.color || annotationColor.value;
            annotationOpacity.value = annotation.style.opacity ?? annotationOpacity.value;
            annotationAuthor.value = annotation.author_id || annotationDefaultAuthor || 'viewer';
        }

        async function applyControlsToSelected() {
            if (!selectedAnnotation) return;
            selectedAnnotation.label = annotationLabel.value.trim() || null;
            selectedAnnotation.style = currentStyle();
            renderAnnotations();
            await updateAnnotation(selectedAnnotation);
        }

        function renderAnnotations() {
            const rect = annotationCanvas.getBoundingClientRect();
            annotationCtx.clearRect(0, 0, rect.width, rect.height);
            for (const annotation of annotations) {
                drawGeometry(annotation.geometry, annotation.style, annotation === selectedAnnotation);
            }
            if (draft) drawGeometry(draft.geometry, { ...currentStyle(), opacity: 0.35 }, true);
            if (editMode && selectedAnnotation) drawSelectionBox(selectedAnnotation);
            renderMinimap();
        }

        async function finishDraft() {
            if (!draft || !draft.geometry) {
                setStatus('Nothing to finish');
                return;
            }
            const minPoints = draft.geometry.kind === 'polygon' ? 3 : 2;
            if (!['polygon', 'polyline'].includes(draft.geometry.kind) || draft.geometry.points.length < minPoints) {
                setStatus(draft.geometry.kind === 'polygon' ? 'Polygon needs 3 points' : 'Path needs 2 points');
                return;
            }
            const geometry = draft.geometry;
            draft = null;
            await saveAnnotation(geometry);
            renderAnnotations();
        }

        function cancelDraft() {
            draft = null;
            dragStart = null;
            setStatus('Drawing canceled');
            renderAnnotations();
        }

        function drawGeometry(geometry, style, selected) {
            annotationCtx.save();
            annotationCtx.strokeStyle = style.color || '#ff3366';
            annotationCtx.fillStyle = hexToRgba(style.color || '#ff3366', style.opacity ?? 0.55);
            annotationCtx.globalAlpha = selected ? 1 : 0.95;
            annotationCtx.lineWidth = selected ? 3 : 2;
            annotationCtx.beginPath();
            if (geometry.kind === 'point') {
                const p = imageToCanvas(geometry.point);
                annotationCtx.arc(p.x, p.y, pointCanvasRadius(style, geometry.point, selected), 0, Math.PI * 2);
                annotationCtx.fill();
                annotationCtx.stroke();
            } else if (geometry.kind === 'rectangle') {
                const p = imageToCanvas({ x: geometry.x, y: geometry.y });
                const q = imageToCanvas({ x: geometry.x + geometry.width, y: geometry.y + geometry.height });
                annotationCtx.rect(p.x, p.y, q.x - p.x, q.y - p.y);
                annotationCtx.fill();
                annotationCtx.stroke();
            } else if (geometry.kind === 'circle' || geometry.kind === 'ellipse') {
                const c = imageToCanvas(geometry.center);
                const rxPoint = imageToCanvas({ x: geometry.center.x + (geometry.radius_x || geometry.radius), y: geometry.center.y });
                const ryPoint = imageToCanvas({ x: geometry.center.x, y: geometry.center.y + (geometry.radius_y || geometry.radius) });
                annotationCtx.ellipse(c.x, c.y, Math.abs(rxPoint.x - c.x), Math.abs(ryPoint.y - c.y), 0, 0, Math.PI * 2);
                annotationCtx.fill();
                annotationCtx.stroke();
            } else if (geometry.kind === 'line') {
                drawPath([geometry.start, geometry.end], false);
            } else if (geometry.kind === 'polyline') {
                drawPath(geometry.points, false);
            } else if (geometry.kind === 'polygon') {
                drawPath(geometry.points, true);
                annotationCtx.fill();
            }
            annotationCtx.restore();
        }

        function pointCanvasRadius(style, imagePoint, selected) {
            const imageRadius = style.point_radius;
            if (!imageRadius) return selected ? 7 : 5;
            const center = imageToCanvas(imagePoint);
            const edge = imageToCanvas({ x: imagePoint.x + imageRadius, y: imagePoint.y });
            return Math.max(selected ? 7 : 5, Math.abs(edge.x - center.x));
        }

        function drawSelectionBox(annotation) {
            if (annotation.annotation_type === 'point') {
                drawPointSelection(annotation);
                return;
            }
            const box = annotation.bbox;
            const topLeft = imageToCanvas({ x: box.x, y: box.y });
            const bottomRight = imageToCanvas({ x: box.x + box.width, y: box.y + box.height });
            const x = Math.min(topLeft.x, bottomRight.x);
            const y = Math.min(topLeft.y, bottomRight.y);
            const width = Math.abs(bottomRight.x - topLeft.x);
            const height = Math.abs(bottomRight.y - topLeft.y);
            annotationCtx.save();
            annotationCtx.strokeStyle = '#ffffff';
            annotationCtx.lineWidth = 1;
            annotationCtx.setLineDash([4, 4]);
            annotationCtx.strokeRect(x, y, Math.max(1, width), Math.max(1, height));
            annotationCtx.setLineDash([]);
            for (const handle of resizeHandlePoints(annotation)) {
                annotationCtx.fillStyle = '#ffffff';
                annotationCtx.strokeStyle = annotation.style.color || '#ff3366';
                annotationCtx.lineWidth = 2;
                annotationCtx.fillRect(handle.canvas.x - 5, handle.canvas.y - 5, 10, 10);
                annotationCtx.strokeRect(handle.canvas.x - 5, handle.canvas.y - 5, 10, 10);
            }
            annotationCtx.restore();
        }

        function drawPointSelection(annotation) {
            const point = annotation.geometry.point;
            const center = imageToCanvas(point);
            const radius = pointCanvasRadius(annotation.style, point, true);
            annotationCtx.save();
            annotationCtx.strokeStyle = '#ffffff';
            annotationCtx.lineWidth = 1;
            annotationCtx.setLineDash([4, 4]);
            annotationCtx.strokeRect(center.x - radius, center.y - radius, radius * 2, radius * 2);
            annotationCtx.setLineDash([]);
            const handle = { x: center.x + radius, y: center.y - radius };
            annotationCtx.fillStyle = '#ffffff';
            annotationCtx.strokeStyle = annotation.style.color || '#ff3366';
            annotationCtx.lineWidth = 2;
            annotationCtx.fillRect(handle.x - 5, handle.y - 5, 10, 10);
            annotationCtx.strokeRect(handle.x - 5, handle.y - 5, 10, 10);
            annotationCtx.restore();
        }

        function drawPath(points, closed) {
            if (!points.length) return;
            const first = imageToCanvas(points[0]);
            annotationCtx.moveTo(first.x, first.y);
            for (const point of points.slice(1)) {
                const p = imageToCanvas(point);
                annotationCtx.lineTo(p.x, p.y);
            }
            if (closed) annotationCtx.closePath();
            annotationCtx.stroke();
        }

        function renderMinimap() {
            annotationMinimapCtx.clearRect(0, 0, 184, 132);
            annotationMinimapCtx.fillStyle = 'rgba(255,255,255,0.06)';
            annotationMinimapCtx.fillRect(0, 0, 184, 132);
            const sx = 184 / tileSource.width;
            const sy = 132 / tileSource.height;
            for (const annotation of annotations) {
                const box = annotation.bbox;
                annotationMinimapCtx.strokeStyle = annotation.style.color || '#ff3366';
                annotationMinimapCtx.strokeRect(box.x * sx, box.y * sy, Math.max(2, box.width * sx), Math.max(2, box.height * sy));
            }
            const bounds = viewer.viewport.viewportToImageRectangle(viewer.viewport.getBounds(true));
            annotationMinimapCtx.strokeStyle = '#ffffff';
            annotationMinimapCtx.lineWidth = 1;
            annotationMinimapCtx.strokeRect(bounds.x * sx, bounds.y * sy, bounds.width * sx, bounds.height * sy);
        }

        function hexToRgba(hex, opacity) {
            const raw = hex.replace('#', '');
            const r = parseInt(raw.slice(0, 2), 16);
            const g = parseInt(raw.slice(2, 4), 16);
            const b = parseInt(raw.slice(4, 6), 16);
            return `rgba(${r}, ${g}, ${b}, ${opacity})`;
        }

        function geometryFromDrag(tool, start, end) {
            const x = Math.min(start.x, end.x);
            const y = Math.min(start.y, end.y);
            const width = Math.abs(end.x - start.x);
            const height = Math.abs(end.y - start.y);
            if (tool === 'rectangle') return { kind: 'rectangle', x, y, width: Math.max(1, width), height: Math.max(1, height) };
            if (tool === 'line') return { kind: 'line', start, end };
            if (tool === 'ellipse') return { kind: 'ellipse', center: { x: x + width / 2, y: y + height / 2 }, radius_x: Math.max(1, width / 2), radius_y: Math.max(1, height / 2) };
            return null;
        }

        function screenPixelsToImageTolerance(imagePoint, screenPixels) {
            const canvasPoint = imageToCanvas(imagePoint);
            const shiftedPixel = new OpenSeadragon.Point(canvasPoint.x + screenPixels, canvasPoint.y);
            const shiftedViewport = viewer.viewport.pointFromPixel(shiftedPixel, true);
            const shiftedImage = viewer.viewport.viewportToImageCoordinates(shiftedViewport);
            return Math.max(1, Math.abs(shiftedImage.x - imagePoint.x));
        }

        function findAnnotationAt(imagePoint) {
            const tolerance = screenPixelsToImageTolerance(imagePoint, 12);
            return [...annotations].reverse().find(annotation => {
                const b = annotation.bbox;
                return imagePoint.x >= b.x - tolerance && imagePoint.x <= b.x + b.width + tolerance
                    && imagePoint.y >= b.y - tolerance && imagePoint.y <= b.y + b.height + tolerance;
            });
        }

        function resizeHandlePoints(annotation) {
            if (annotation.annotation_type === 'point') {
                const point = annotation.geometry.point;
                const center = imageToCanvas(point);
                const radius = pointCanvasRadius(annotation.style, point, true);
                return [{
                    name: 'point-radius',
                    image: point,
                    canvas: { x: center.x + radius, y: center.y - radius }
                }];
            }
            const b = annotation.bbox;
            return [
                { name: 'nw', image: { x: b.x, y: b.y } },
                { name: 'ne', image: { x: b.x + b.width, y: b.y } },
                { name: 'se', image: { x: b.x + b.width, y: b.y + b.height } },
                { name: 'sw', image: { x: b.x, y: b.y + b.height } }
            ].map(handle => ({ ...handle, canvas: imageToCanvas(handle.image) }));
        }

        function findResizeHandle(annotation, event) {
            if (!annotation) return null;
            const rect = annotationCanvas.getBoundingClientRect();
            const canvasPoint = { x: event.clientX - rect.left, y: event.clientY - rect.top };
            return resizeHandlePoints(annotation).find(handle =>
                Math.abs(canvasPoint.x - handle.canvas.x) <= 8
                && Math.abs(canvasPoint.y - handle.canvas.y) <= 8
            )?.name || null;
        }

        function translateGeometry(geometry, dx, dy) {
            const move = p => ({ x: p.x + dx, y: p.y + dy });
            if (geometry.kind === 'point') geometry.point = move(geometry.point);
            if (geometry.kind === 'rectangle') { geometry.x += dx; geometry.y += dy; }
            if (geometry.kind === 'circle' || geometry.kind === 'ellipse') geometry.center = move(geometry.center);
            if (geometry.kind === 'line') { geometry.start = move(geometry.start); geometry.end = move(geometry.end); }
            if (geometry.kind === 'polygon' || geometry.kind === 'polyline') geometry.points = geometry.points.map(move);
        }

        function recomputeBbox(annotation) {
            const g = annotation.geometry;
            if (g.kind === 'point') {
                const radius = annotation.style.point_radius || 0;
                annotation.bbox = { x: g.point.x - radius, y: g.point.y - radius, width: radius * 2, height: radius * 2 };
            }
            if (g.kind === 'rectangle') annotation.bbox = { x: g.x, y: g.y, width: g.width, height: g.height };
            if (g.kind === 'circle') annotation.bbox = { x: g.center.x - g.radius, y: g.center.y - g.radius, width: g.radius * 2, height: g.radius * 2 };
            if (g.kind === 'ellipse') annotation.bbox = { x: g.center.x - g.radius_x, y: g.center.y - g.radius_y, width: g.radius_x * 2, height: g.radius_y * 2 };
            if (g.kind === 'line') bboxFromPoints(annotation, [g.start, g.end]);
            if (g.kind === 'polygon' || g.kind === 'polyline') bboxFromPoints(annotation, g.points);
        }

        function bboxFromPoints(annotation, points) {
            const xs = points.map(p => p.x);
            const ys = points.map(p => p.y);
            const minX = Math.min(...xs), minY = Math.min(...ys);
            annotation.bbox = { x: minX, y: minY, width: Math.max(...xs) - minX, height: Math.max(...ys) - minY };
        }

        function cloneGeometry(geometry) {
            return JSON.parse(JSON.stringify(geometry));
        }

        function normalizeBoxFromHandle(originalBox, handle, point) {
            let left = originalBox.x;
            let right = originalBox.x + originalBox.width;
            let top = originalBox.y;
            let bottom = originalBox.y + originalBox.height;
            if (handle.includes('w')) left = point.x;
            if (handle.includes('e')) right = point.x;
            if (handle.includes('n')) top = point.y;
            if (handle.includes('s')) bottom = point.y;
            const minX = Math.min(left, right);
            const minY = Math.min(top, bottom);
            return {
                x: minX,
                y: minY,
                width: Math.max(1, Math.abs(right - left)),
                height: Math.max(1, Math.abs(bottom - top))
            };
        }

        function scalePoint(point, fromBox, toBox) {
            const sx = toBox.width / Math.max(1, fromBox.width);
            const sy = toBox.height / Math.max(1, fromBox.height);
            return {
                x: toBox.x + (point.x - fromBox.x) * sx,
                y: toBox.y + (point.y - fromBox.y) * sy
            };
        }

        function resizeGeometry(originalGeometry, originalBox, nextBox) {
            const geometry = cloneGeometry(originalGeometry);
            if (geometry.kind === 'rectangle') {
                geometry.x = nextBox.x;
                geometry.y = nextBox.y;
                geometry.width = nextBox.width;
                geometry.height = nextBox.height;
            } else if (geometry.kind === 'ellipse') {
                geometry.center = { x: nextBox.x + nextBox.width / 2, y: nextBox.y + nextBox.height / 2 };
                geometry.radius_x = nextBox.width / 2;
                geometry.radius_y = nextBox.height / 2;
            } else if (geometry.kind === 'circle') {
                geometry.center = { x: nextBox.x + nextBox.width / 2, y: nextBox.y + nextBox.height / 2 };
                geometry.radius = Math.max(nextBox.width, nextBox.height) / 2;
            } else if (geometry.kind === 'line') {
                geometry.start = scalePoint(geometry.start, originalBox, nextBox);
                geometry.end = scalePoint(geometry.end, originalBox, nextBox);
            } else if (geometry.kind === 'polygon' || geometry.kind === 'polyline') {
                geometry.points = geometry.points.map(point => scalePoint(point, originalBox, nextBox));
            }
            return geometry;
        }

        function resizePointAnnotation(annotation, point) {
            const center = annotation.geometry.point;
            const dx = point.x - center.x;
            const dy = point.y - center.y;
            annotation.style.point_radius = Math.max(1, Math.sqrt(dx * dx + dy * dy));
            recomputeBbox(annotation);
        }

        document.querySelectorAll('.annotation-panel .tool').forEach(button => {
            button.addEventListener('click', () => {
                document.querySelectorAll('.annotation-panel .tool').forEach(b => b.classList.remove('active'));
                button.classList.add('active');
                activeTool = button.dataset.tool;
                editMode = false;
                document.getElementById('annotation-edit').classList.remove('active');
                draft = null;
                selectedAnnotation = null;
                annotationLabel.value = '';
                annotationAuthor.value = annotationDefaultAuthor || 'viewer';
                setStatus('');
                renderAnnotations();
            });
        });

        document.getElementById('annotation-edit').addEventListener('click', event => {
            editMode = !editMode;
            event.currentTarget.classList.toggle('active', editMode);
            draft = null;
            selectedAnnotation = null;
            annotationLabel.value = '';
            annotationAuthor.value = annotationDefaultAuthor || 'viewer';
            setStatus(editMode ? 'Move mode' : '');
            renderAnnotations();
        });
        document.getElementById('annotation-refresh').addEventListener('click', () => loadAnnotations(true));
        annotationFinish.addEventListener('click', finishDraft);
        annotationCancel.addEventListener('click', cancelDraft);
        annotationLabel.addEventListener('change', applyControlsToSelected);
        annotationColor.addEventListener('change', applyControlsToSelected);
        annotationOpacity.addEventListener('change', applyControlsToSelected);

        annotationCanvas.addEventListener('mousedown', event => {
            if (event.detail > 1 && draft && ['polygon', 'polyline'].includes(draft.geometry.kind)) {
                return;
            }
            const point = canvasToImage(event);
            if (editMode) {
                resizeHandle = findResizeHandle(selectedAnnotation, event);
                if (resizeHandle) {
                    editOperation = 'resize';
                    dragStart = point;
                    resizeStart = {
                        handle: resizeHandle,
                        bbox: { ...selectedAnnotation.bbox },
                        geometry: cloneGeometry(selectedAnnotation.geometry)
                    };
                    setStatus(`Resize ${resizeHandle}`);
                    renderAnnotations();
                    return;
                }
                selectedAnnotation = findAnnotationAt(point);
                dragStart = selectedAnnotation ? point : null;
                editOperation = selectedAnnotation ? 'move' : null;
                if (selectedAnnotation) {
                    populateControlsFromAnnotation(selectedAnnotation);
                    setStatus(`Selected ${selectedAnnotation.annotation_type}; drag inside to move, drag corners to resize`);
                } else {
                    setStatus('No annotation selected');
                }
                renderAnnotations();
                return;
            }
            if (activeTool === 'point') {
                saveAnnotation({ kind: 'point', point });
                return;
            }
            if (activeTool === 'polygon' || activeTool === 'polyline') {
                if (!draft) draft = { geometry: { kind: activeTool, points: [] } };
                draft.geometry.points.push(point);
                setStatus(`${draft.geometry.kind}: ${draft.geometry.points.length} point${draft.geometry.points.length === 1 ? '' : 's'}`);
                renderAnnotations();
                return;
            }
            dragStart = point;
            draft = { geometry: geometryFromDrag(activeTool, point, point) };
        });

        annotationCanvas.addEventListener('mousemove', event => {
            const point = canvasToImage(event);
            const hovered = findAnnotationAt(point);
            if (hovered && hovered.label) {
                annotationTooltip.textContent = hovered.label;
                annotationTooltip.style.display = 'block';
                annotationTooltip.style.left = event.clientX + 12 + 'px';
                annotationTooltip.style.top = event.clientY + 12 + 'px';
            } else {
                annotationTooltip.style.display = 'none';
            }
            if (editMode && selectedAnnotation && dragStart && editOperation === 'resize' && resizeStart) {
                if (selectedAnnotation.annotation_type === 'point') {
                    resizePointAnnotation(selectedAnnotation, point);
                } else {
                    const nextBox = normalizeBoxFromHandle(resizeStart.bbox, resizeStart.handle, point);
                    selectedAnnotation.geometry = resizeGeometry(resizeStart.geometry, resizeStart.bbox, nextBox);
                    recomputeBbox(selectedAnnotation);
                }
                renderAnnotations();
                return;
            }
            if (editMode && selectedAnnotation && dragStart && editOperation === 'move') {
                translateGeometry(selectedAnnotation.geometry, point.x - dragStart.x, point.y - dragStart.y);
                recomputeBbox(selectedAnnotation);
                dragStart = point;
                renderAnnotations();
                return;
            }
            if (dragStart && draft && !editMode) {
                draft.geometry = geometryFromDrag(activeTool, dragStart, point);
                renderAnnotations();
            }
        });

        annotationCanvas.addEventListener('mouseup', async () => {
            if (editMode && selectedAnnotation) {
                await updateAnnotation(selectedAnnotation);
                dragStart = null;
                editOperation = null;
                resizeHandle = null;
                resizeStart = null;
                return;
            }
            if (draft && draft.geometry && !['polygon', 'polyline'].includes(activeTool)) {
                await saveAnnotation(draft.geometry);
                draft = null;
                dragStart = null;
                renderAnnotations();
            }
        });

        annotationCanvas.addEventListener('dblclick', async () => {
            await finishDraft();
        });

        document.addEventListener('keydown', async event => {
            const target = event.target;
            const isTextInput = target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
            if (isTextInput && event.key !== 'Escape') return;
            if (event.key === 'Enter' && draft && ['polygon', 'polyline'].includes(draft.geometry.kind)) {
                event.preventDefault();
                await finishDraft();
            }
            if (event.key === 'Escape' && draft) {
                event.preventDefault();
                cancelDraft();
            }
        });

        viewer.addHandler('open', () => {
            resizeAnnotationCanvas();
            loadAnnotations(true);
        });
        viewer.addHandler('animation', renderAnnotations);
        viewer.addHandler('resize', resizeAnnotationCanvas);
        viewer.addHandler('pan', loadAnnotations);
        viewer.addHandler('zoom', loadAnnotations);
        window.addEventListener('resize', resizeAnnotationCanvas);
    "#;

    script
        .replace(
            "__BASE_URL__",
            &js_string_escape(base_url.trim_end_matches('/')),
        )
        .replace("__SLIDE_ID__", &js_string_escape(encoded_slide_id))
        .replace("__AUTH_QUERY__", &js_string_escape(auth_query))
        .replace("__AUTHOR_ID__", &js_string_escape(author_id))
}

fn js_string_escape(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::handlers::LevelMetadataResponse;

    fn test_metadata() -> SlideMetadataResponse {
        SlideMetadataResponse {
            slide_id: "test.svs".to_string(),
            format: "Aperio SVS".to_string(),
            width: 50000,
            height: 40000,
            level_count: 3,
            levels: vec![
                LevelMetadataResponse {
                    level: 0,
                    width: 50000,
                    height: 40000,
                    tile_width: 256,
                    tile_height: 256,
                    tiles_x: 196,
                    tiles_y: 157,
                    downsample: 1.0,
                },
                LevelMetadataResponse {
                    level: 1,
                    width: 12500,
                    height: 10000,
                    tile_width: 256,
                    tile_height: 256,
                    tiles_x: 49,
                    tiles_y: 40,
                    downsample: 4.0,
                },
                LevelMetadataResponse {
                    level: 2,
                    width: 3125,
                    height: 2500,
                    tile_width: 256,
                    tile_height: 256,
                    tiles_x: 13,
                    tiles_y: 10,
                    downsample: 16.0,
                },
            ],
        }
    }

    #[test]
    fn test_generate_viewer_html_contains_slide_info() {
        let metadata = test_metadata();
        let html =
            generate_viewer_html("test.svs", &metadata, "http://localhost:3000", "", "viewer");

        assert!(html.contains("test.svs"));
        assert!(html.contains("50000"));
        assert!(html.contains("40000"));
        // Level count is wrapped in span: <span>3</span> pyramid levels
        assert!(html.contains(">3</span> pyramid levels"));
        assert!(html.contains("Aperio SVS"));
    }

    #[test]
    fn test_generate_viewer_html_contains_openseadragon() {
        let metadata = test_metadata();
        let html =
            generate_viewer_html("test.svs", &metadata, "http://localhost:3000", "", "viewer");

        assert!(html.contains("openseadragon"));
        assert!(html.contains("OpenSeadragon"));
    }

    #[test]
    fn test_generate_viewer_html_contains_tile_url() {
        let metadata = test_metadata();
        let html =
            generate_viewer_html("test.svs", &metadata, "http://localhost:3000", "", "viewer");

        assert!(html.contains("/tiles/test.svs/"));
        assert!(html.contains(".jpg"));
    }

    #[test]
    fn test_generate_viewer_html_with_auth_query() {
        let metadata = test_metadata();
        let html = generate_viewer_html(
            "test.svs",
            &metadata,
            "http://localhost:3000",
            "?exp=123&sig=abc",
            "viewer",
        );

        assert!(html.contains("?exp=123&sig=abc"));
    }

    #[test]
    fn test_generate_viewer_html_encodes_slide_id() {
        let metadata = test_metadata();
        let html = generate_viewer_html(
            "folder/sub folder/test.svs",
            &metadata,
            "http://localhost:3000",
            "",
            "viewer",
        );

        // Should URL-encode the slide_id in tile URLs
        assert!(html.contains("folder%2Fsub%20folder%2Ftest.svs"));
    }

    #[test]
    fn test_generate_viewer_html_contains_level_dimensions() {
        let metadata = test_metadata();
        let html =
            generate_viewer_html("test.svs", &metadata, "http://localhost:3000", "", "viewer");

        // Should contain level dimension objects
        assert!(html.contains("width: 50000, height: 40000"));
        assert!(html.contains("width: 12500, height: 10000"));
        assert!(html.contains("width: 3125, height: 2500"));
    }

    #[test]
    fn test_html_escape_basic() {
        assert_eq!(html_escape("hello"), "hello");
        assert_eq!(html_escape(""), "");
        assert_eq!(html_escape("test.svs"), "test.svs");
    }

    #[test]
    fn test_html_escape_special_chars() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(html_escape("it's"), "it&#x27;s");
        assert_eq!(
            html_escape("<script>alert('xss')</script>"),
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
    }

    #[test]
    fn test_generate_viewer_html_escapes_xss_in_slide_id() {
        let mut metadata = test_metadata();
        metadata.slide_id = "<script>alert(1)</script>".to_string();

        let html = generate_viewer_html(
            "<script>alert(1)</script>",
            &metadata,
            "http://localhost:3000",
            "",
            "viewer",
        );

        // The literal script tag should NOT appear unescaped
        assert!(!html.contains("<script>alert(1)</script>"));
        // The escaped version should appear
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }

    #[test]
    fn test_generate_viewer_html_escapes_xss_in_format() {
        let mut metadata = test_metadata();
        metadata.format = "<img onerror=alert(1)>".to_string();

        let html =
            generate_viewer_html("test.svs", &metadata, "http://localhost:3000", "", "viewer");

        // The literal img tag should NOT appear unescaped
        assert!(!html.contains("<img onerror=alert(1)>"));
        // The escaped version should appear in the format badge
        assert!(html.contains("&lt;img onerror=alert(1)&gt;"));
    }
}
