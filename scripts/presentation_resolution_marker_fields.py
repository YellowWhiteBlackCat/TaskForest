"""The one Python-side spelling of the shared resolution marker field set.

The Rust owner is `taskmanager_ui_contract::presentation::resolution`.
Capture validators use this list to reject an appended resolution record whose
field set has drifted; they never use it to re-derive a resolution verdict.
"""

RESOLUTION_MARKER_FIELDS = (
    "profile",
    "vertical",
    "padding",
    "width_input",
    "page_content_height",
    "control_extent",
    "hit_target",
    "control_height",
    "body_font_size",
    "renderer_scale",
    "row_padding_y",
    "line_height",
    "rung",
    "inventory",
    "details",
    "device_nav",
    "sidebar_visible",
    "persisted_sidebar_width",
    "workspace_width",
    "content_height",
    "sidebar_width",
    "stats_width",
    "main_width",
)
